use chrono::Local;
use getopts::Options;
use std::env;
use std::fs::{create_dir_all, File};
use std::io::{BufWriter, Write};
use std::net::{ToSocketAddrs, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Copy, Clone)]
struct Count {
    inb: u64,
    outb: u64,
    lost: u64,
}

fn print_usage(program: &str, opts: &Options) {
    let brief = format!(
        r#"Echo benchmark.
                            Usage:
                                      {program} [ -a <address> ] [ -l <length> ] [ -c <number> ] [ -t <duration> ]
                                                  {program} (-h | --help)
                                                                {program} --version"#,
                                                                program = program
                       );
    print!("{}", opts.usage(&brief));
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag("h", "help", "Print this help.");
    opts.optopt("a", "address", "Target echo server address. Default: 127.0.0.1:12345", "<address>");
    opts.optopt("l", "length", "Test message length. Default: 512", "<length>");
    opts.optopt("t", "duration", "Test duration in seconds. Default: 60", "<duration>");
    opts.optopt("c", "number", "Test connection number. Default: 50", "<number>");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            eprintln!("{}", f.to_string());
            print_usage(&program, &opts);
            return;
        }
    };

    if matches.opt_present("h") {
        print_usage(&program, &opts);
        return;
    }

    let length = matches.opt_str("l").unwrap_or_default().parse::<usize>().unwrap_or(512);
    let duration = matches.opt_str("t").unwrap_or_default().parse::<u64>().unwrap_or(60);
    let number = matches.opt_str("c").unwrap_or_default().parse::<u32>().unwrap_or(50);
    let address = matches.opt_str("a").unwrap_or_else(|| "127.0.0.1:12345".to_string());

    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));

    let now = Local::now();
    let timestamp_folder_name = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let base_output_path = PathBuf::from("latency_udp").join(timestamp_folder_name);
    create_dir_all(&base_output_path).expect("Failed to create latency output dir");
    let base_output_path_arc = Arc::new(base_output_path);

    println!(
        "Starting UDP benchmark with {} connections, {} byte messages, for {} seconds, to {}",
        number, length, duration, address
        );

    for id in 0..number {
        let tx_clone = tx.clone();
        let stop_clone = Arc::clone(&stop);
        let address_clone = address.clone();
        let output_path_for_thread = Arc::clone(&base_output_path_arc);

        thread::spawn(move || {
            let socket = match UdpSocket::bind("0.0.0.0:0") {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Thread {}: Failed to bind socket: {}", id, e);
                    let _ = tx_clone.send(Count { inb: 0, outb: 0, lost: 0 });
                    return;
                }
            };

            if socket.connect(&address_clone).is_err() {
                eprintln!("Thread {}: connect() failed for {}", id, address_clone);
                let _ = tx_clone.send(Count { inb: 0, outb: 0, lost: 0 });
                return;
            }

            socket.set_read_timeout(Some(Duration::from_millis(1000))).ok();
            socket.set_write_timeout(Some(Duration::from_millis(1000))).ok();

            let mut sum = Count { inb: 0, outb: 0, lost: 0 };
            let mut out_buf = vec![0u8; length];
            if length > 0 {
                out_buf[length - 1] = b'\n';
            }
            let mut in_buf = vec![0u8; length];
            let mut latencies = Vec::new();

            let start_time = Instant::now();
            while !stop_clone.load(Ordering::Relaxed)
                && start_time.elapsed().as_secs() < duration + 60
                {
                    let send_time = Instant::now();
                    let mut success = false;
                    for attempt in 0..3 {
                        if socket.send(&out_buf).is_ok() {
                            sum.outb += 1;
                        } else {
                            eprintln!("Thread {}: send() failed (attempt {})", id, attempt + 1);
                            continue;
                        }

                        match socket.recv(&mut in_buf) {
                            Ok(received) if received == length => {
                                sum.inb += 1;
                                latencies.push(send_time.elapsed());
                                success = true;
                                break;
                            }
                            Ok(n) => {
                                eprintln!("Thread {}: Incomplete recv: {} bytes", id, n);
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                eprintln!("Thread {}: recv() timeout (attempt {})", id, attempt + 1);
                            }
                            Err(e) => {
                                eprintln!("Thread {}: recv() failed: {} (attempt {})", id, e, attempt + 1);
                            }
                        }
                    }

                    if !success {
                        sum.lost += 1;
                    }
                }

            let file_path = output_path_for_thread.join(format!("latency_thread_{}.txt", id));
            if let Ok(file) = File::create(&file_path) {
                let mut writer = BufWriter::new(file);
                for lat in latencies {
                    let _ = writeln!(writer, "{}", lat.as_nanos());
                }
            }

            let _ = tx_clone.send(sum);
        });
    }

    thread::sleep(Duration::from_secs(duration + 60));
    stop.store(true, Ordering::Relaxed);

    let mut total = Count { inb: 0, outb: 0, lost: 0 };
    for _ in 0..number {
        if let Ok(c) = rx.recv_timeout(Duration::from_secs(5)) {
            total.inb += c.inb;
            total.outb += c.outb;
            total.lost += c.lost;
        }
    }

    println!("Total Sent: {}, Total Received: {}, Total Lost: {}", total.outb, total.inb, total.lost);
}

