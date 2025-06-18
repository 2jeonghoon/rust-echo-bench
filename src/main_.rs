use std::env;
use std::io::{Read, Write, BufWriter};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};
use std::fs::{File, create_dir_all};
use chrono::Local;
use std::path::PathBuf;

fn print_usage(program: &str, opts: &getopts::Options) {
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

struct Count {
    inb: u64,
    outb: u64,
}

fn main() {
    let args: Vec<_> = env::args().collect();
    let program = args[0].clone();

    let mut opts = getopts::Options::new();
    opts.optflag("h", "help", "Print this help.");
    opts.optopt(
        "a",
        "address",
        "Target echo server address. Default: 127.0.0.1:12345",
        "<address>",
    );
    opts.optopt(
        "l",
        "length",
        "Test message length. Default: 512",
        "<length>",
    );
    opts.optopt(
        "t",
        "duration",
        "Test duration in seconds. Default: 60",
        "<duration>",
    );
    opts.optopt(
        "c",
        "number",
        "Test connection number. Default: 50",
        "<number>",
    );

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

    let length = matches
        .opt_str("length")
        .unwrap_or_default()
        .parse::<usize>()
        .unwrap_or(512);
    let duration = matches
        .opt_str("duration")
        .unwrap_or_default()
        .parse::<u64>()
        .unwrap_or(60);
    let number = matches
        .opt_str("number")
        .unwrap_or_default()
        .parse::<u32>()
        .unwrap_or(50);
    let address = matches
        .opt_str("address")
        .unwrap_or_else(|| "127.0.0.1:12345".to_string());

    let (tx, rx) = mpsc::channel();

    let stop = Arc::new(AtomicBool::new(false));
    let control = Arc::downgrade(&stop);

    let now = Local::now();
    let timestamp_folder_name = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let base_output_path = PathBuf::from("latency").join(timestamp_folder_name);

    if let Err(e) = create_dir_all(&base_output_path) {
        eprintln!("Failed to create output directory {:?}: {}", base_output_path, e);

        return;
    }

    let base_output_path_arc = Arc::new(base_output_path);

    for _id in 0..number {
        let tx = tx.clone();
        let address = address.clone();
        let stop = stop.clone();
        let length = length;
        let output_path_for_thread = Arc::clone(&base_output_path_arc);

        thread::spawn(move || {
            let id = _id;
            let mut sum = Count { inb: 0, outb: 0 };
            let mut out_buf: Vec<u8> = vec![0; length];
            out_buf[length - 1] = b'\n';
            let mut in_buf: Vec<u8> = vec![0; length];
            let mut stream = TcpStream::connect(&*address).unwrap();

            let mut latencies: Vec<Duration> = Vec::new();

            let _ = stream.set_read_timeout(Some(Duration::new(5, 0)));

            loop {
                if (*stop).load(Ordering::Relaxed) {
                    break;
                }

                let start_time = Instant::now();

                match stream.write_all(&out_buf) {
                    Err(e) => {
                        eprintln!("Write operation failed: Error Kind: {:?}, Message: {}", e.kind(), e);
                        break;
                    }
                    Ok(_) => sum.outb += 1,
                }

                if (*stop).load(Ordering::Relaxed) {
                    break;
                }

                match stream.read(&mut in_buf) {
                    Err(e) => {
                        eprintln!("{}: Read operation failed: Error Kind: {:?}, Message: {}", id, e.kind(), e);
                        continue;
                    },
                    Ok(m) => {
                        if m == 0 || m != length {
                            println!("Read error! length={}", m);
                            continue;
                        }
                    }
                };

                let end_time = Instant::now();

                latencies.push(end_time.duration_since(start_time));

                sum.inb += 1;
            }

            let latency_file_name = format!("latency_{}.txt", id);
            let final_file_path = output_path_for_thread.join(&latency_file_name);
            match File::create(&final_file_path) {
                Ok(file) => {
                    let mut writer = BufWriter::new(file);
                    println!("Thread {}: Writing {} latencies to {}", id, latencies.len(), latency_file_name);
                    for latency in latencies {
                        if writeln!(writer, "{}", latency.as_micros()).is_err() {
                            eprintln!("Thread {}: Eror writing latency to file {}", id, latency_file_name);
                            break;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Thread {}: Failed to create latency file {}: {}", id, latency_file_name, e);
                }
            }
            
            tx.send(sum).unwrap();
        });

        thread::sleep(Duration::from_millis(10));
    }

    thread::sleep(Duration::from_secs(duration));

    match control.upgrade() {
        Some(stop) => (*stop).store(true, Ordering::Relaxed),
        None => println!("Sorry, but all threads died already."),
    }

    let mut sum = Count { inb: 0, outb: 0 };
    for _ in 0..number {
        let c: Count = rx.recv().unwrap();
        sum.inb += c.inb;
        sum.outb += c.outb;
    }
    println!("Benchmarking: {}", address);
    println!(
        "{} clients, running {} bytes, {} sec.",
        number, length, duration
    );
    println!();
    println!(
        "Speed: {} request/sec, {} response/sec",
        sum.outb / duration,
        sum.inb / duration
    );
    println!("Requests: {}", sum.outb);
    println!("Responses: {}", sum.inb);
}
