use getopts::Options;
use std::env;
use std::net::{UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration};
use rand::{Rng};

#[derive(Debug, Copy, Clone)]
struct Count {
    inb: u64,
    outb: u64,
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

    println!(
        "Starting UDP benchmark with {} connections, {} byte messages, for {} seconds, to {}",
        number, length, duration, address
        );

    for id in 0..number {
        let tx_clone = tx.clone();
        let stop_clone = Arc::clone(&stop);
        let address_clone = address.clone();
        thread::spawn(move || {
            let socket = match UdpSocket::bind("0.0.0.0:0") {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Thread {}: Failed to bind socket: {}", id, e);
                    let _ = tx_clone.send(Count { inb: 0, outb: 0});
                    return;
                }
            };

            if socket.connect(&address_clone).is_err() {
                eprintln!("Thread {}: connect() failed for {}", id, address_clone);
                let _ = tx_clone.send(Count { inb: 0, outb: 0});
                return;
            }

            socket.set_read_timeout(Some(Duration::from_millis(100000))).ok();
            socket.set_write_timeout(Some(Duration::from_millis(100000))).ok();

            let socket_rx = socket.try_clone().expect("Socket clone failed");
            let stop_rx = Arc::clone(&stop_clone);

            let inb_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let inb_counter_clone = Arc::clone(&inb_counter);

            thread::spawn(move || {
                let mut in_buf = vec![0u8; length];
                while !stop_rx.load(Ordering::Relaxed) {
                    match socket_rx.recv(&mut in_buf) {
                        Ok(_received) => {
                            inb_counter_clone.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            println!("recv timeout 발생");
                        }
                        Err(e) => {
                            eprintln!("recv 에러: {}", e);
                        }
                    }
                }
            });

            let mut outb: u64 = 0;
            let mut rng = rand::thread_rng();
            let mut buf = vec![0u8; length];
            let send_interval = Duration::from_millis(100);

            while !stop_clone.load(Ordering::Relaxed) {
                let msg: String = (0..length - 1).map(|_| rng.sample(rand::distributions::Alphanumeric) as char).collect();

                buf[..length - 1].copy_from_slice(msg.as_bytes());
                buf[length - 1] = b'\n';

                if socket.send(&buf).is_ok() {
                    outb += 1;
                }

                thread::sleep(send_interval);
            }

            let inb = inb_counter.load(Ordering::Relaxed);
            let _ = tx_clone.send(Count {inb, outb});
        });
    }

    thread::sleep(Duration::from_secs(duration));
    stop.store(true, Ordering::Relaxed);

    let mut total = Count { inb: 0, outb: 0 };
    for _ in 0..number {
        if let Ok(c) = rx.recv_timeout(Duration::from_secs(5)) {
            total.inb += c.inb;
            total.outb += c.outb;
        }
    }

    println!("Total Sent: {}, Total Received: {}", total.outb, total.inb);
}

