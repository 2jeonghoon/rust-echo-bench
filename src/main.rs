use chrono::Local;
use std::env;
use std::fs::{create_dir_all, File};
use std::io::{BufWriter, Read, Write}; // BufWriter 추가
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

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

#[derive(Debug, Copy, Clone)] // Debug, Copy, Clone 추가

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
        .opt_str("l") // "length" 대신 "l" 사용 (getopts 설정과 일치)
        .unwrap_or_default()
        .parse::<usize>()
        .unwrap_or(512);
    let duration = matches
        .opt_str("t") // "duration" 대신 "t" 사용
        .unwrap_or_default()
        .parse::<u64>()
        .unwrap_or(60);
    let number = matches
        .opt_str("c") // "number" 대신 "c" 사용
        .unwrap_or_default()
        .parse::<u32>()
        .unwrap_or(50);
    let address = matches
        .opt_str("a") // "address" 대신 "a" 사용
        .unwrap_or_else(|| "127.0.0.1:12345".to_string());

    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));

    let now = Local::now();
    let timestamp_folder_name = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let base_output_path = PathBuf::from("latency").join(timestamp_folder_name);

    if let Err(e) = create_dir_all(&base_output_path) {
        eprintln!(
            "Failed to create output directory {:?}: {}",
            base_output_path, e
            );
        return;
    }
    let base_output_path_arc = Arc::new(base_output_path);

    println!(
        "Starting benchmark with {} connections, {} byte messages, for {} seconds, to {}.",
        number, length, duration, address
        );
    println!("Outputting latency files to: {}", base_output_path_arc.display());


    for thread_id_counter in 0..number {
        let tx_clone = tx.clone(); // tx -> tx_clone
        let address_clone = address.clone(); // address -> address_clone
        let stop_clone = Arc::clone(&stop);
        // length는 Copy 타입이므로 그대로 사용

        let output_path_for_thread = Arc::clone(&base_output_path_arc);

        thread::spawn(move || {
            let id = thread_id_counter;
            let mut sum = Count { inb: 0, outb: 0 };
            let mut out_buf: Vec<u8> = vec![0; length];
            if length > 0 {
                out_buf[length - 1] = b'\n';
            }

            let mut in_buf: Vec<u8> = vec![0; length];

            let mut stream = match TcpStream::connect(&*address_clone) {
                Ok(s) => {
					s.set_nodelay(true).expect("Failed to set TCP_NODELAY");
					s
				},
                Err(e) => {
                    eprintln!(
                        "Thread {}: Failed to connect to {}: {}",
                        id, address_clone, e
                        );
                    let _ = tx_clone.send(Count { inb: 0, outb: 0 });
                    return;
                }
            };

			thread::sleep(Duration::from_secs(15));

            let mut latencies: Vec<Duration> = Vec::new();

           /* if let Err(e) = stream.set_read_timeout(Some(Duration::new(5, 0))) {
                eprintln!("Thread {}: Failed to set read timeout: {}", id, e);
            }
            if let Err(e) = stream.set_write_timeout(Some(Duration::new(5, 0))) {
                eprintln!("Thread {}: Failed to set write timeout: {}", id, e);
            }*/

            'client_loop: loop {
                if stop_clone.load(Ordering::Relaxed) {
                    break 'client_loop;
                }

                let start_time = Instant::now();

                match stream.write_all(&out_buf) {
                    Err(e) => {
                        eprintln!(
                            "Thread {}: Write operation failed: Error Kind: {:?}, Message: {}",
                            id,
                            e.kind(),
                            e
                            );
                        break 'client_loop;
                    }
                    Ok(_) => sum.outb += 1,
                }

                if stop_clone.load(Ordering::Relaxed) {
                    break 'client_loop;
                }

                let mut total_bytes_read_for_this_message = 0;
                let mut read_operation_successful = true;

                while total_bytes_read_for_this_message < length {
                    if stop_clone.load(Ordering::Relaxed) {
                        read_operation_successful = false;
                        break;
                    }
                    match stream.read(&mut in_buf[total_bytes_read_for_this_message..length]) {
                        Ok(0) => {
                            eprintln!("Thread {}: Read operation failed: Connection closed by peer. Expected {} bytes, got {}.", id, length, total_bytes_read_for_this_message);
                            read_operation_successful = false;
                            break;
                        }

                        Ok(n) => {
                            total_bytes_read_for_this_message += n;
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                            eprintln!("Thread {}: Read interrupted, retrying.", id);
                            continue;
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                            eprintln!("Thread {}: Read operation timed out or would block ({} bytes read out of {}). Error: {}", id, total_bytes_read_for_this_message, length, e);
                            read_operation_successful = false;
                            break;
                        }
                        Err(e) => {
                            eprintln!("Thread {}: Read operation failed ({} bytes read out of {}): Error Kind: {:?}, Message: {}", id, total_bytes_read_for_this_message, length, e.kind(), e);
                            read_operation_successful = false;
                            break;
                        }
                    }
                }

                if !read_operation_successful || total_bytes_read_for_this_message != length {
                    if !stop_clone.load(Ordering::Relaxed) {
                        if total_bytes_read_for_this_message != length && read_operation_successful {
                            eprintln!("Thread {}: Incomplete read. Expected {} bytes, but got {} bytes. Aborting client loop.", id, length, total_bytes_read_for_this_message);
                        } else if !read_operation_successful {
                            eprintln!("Thread {}: Aborting client loop for thread {} due to previous read error.", id, id);
                        }
                    }
                    break 'client_loop;
                }

                let end_time = Instant::now();
                latencies.push(end_time.duration_since(start_time));
                sum.inb += 1;

                if &in_buf[..length] != &out_buf[..length] {
                    eprintln!("Thread {}: Data mismatch! Sent != Received", id);
                }
            }

                let latency_file_name = format!("latency_thread_{}.txt", id);
                let final_file_path = output_path_for_thread.join(&latency_file_name);
                match File::create(&final_file_path) {
                    Ok(file) => {
                        let mut writer = BufWriter::new(file);
                        if !latencies.is_empty() {
                            println!(
                                "Thread {}: Writing {} latencies to {}",
                                id,
                                latencies.len(),
                                final_file_path.display()
                                );
                        }
                        for latency in latencies {
                            if writeln!(writer, "{}", latency.as_nanos()).is_err() {
                                eprintln!(
                                    "Thread {}: Error writing latency to file {}",
                                    id,
                                    final_file_path.display()
                                    );
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Thread {}: Failed to create latency file {}: {}",
                            id,
                            final_file_path.display(),
                            e
                        );
                    }
                }
                if let Err(e) = tx_clone.send(sum) {
                    eprintln!("Thread {}: Failed to send sum to main thread: {}", id, e);
                }
            });

            //thread::sleep(Duration::from_millis(1));
        }

        println!(
            "Main: All {} client threads launched. Waiting for {} seconds...",
            number, duration
            );
        thread::sleep(Duration::from_secs(duration+15));

        println!("Main: Time is up. Signalling threads to stop...");
        stop.store(true, Ordering::Relaxed);

        println!("Main: Waiting for threads to finish and collect results...");
        let mut total_sum = Count { inb: 0, outb: 0 };
        let mut received_counts = 0;
        for i in 0..number {
            match rx.recv_timeout(Duration::from_secs(15)) {
                Ok(c) => {
                    total_sum.inb += c.inb;
                    total_sum.outb += c.outb;
                    received_counts += 1;
                }
                Err(e) => {
                    eprintln!(
                        "Main: Failed to receive result from a thread (id might be {} or timed out after 15s): {}. This thread's stats will be missed.",
                        i, e
                        );
                }
            }
        }
        println!("Main: Collected results from {} out of {} threads.", received_counts, number);


        println!("\n--- Benchmark Results ---");
        println!("Target: {}", address);
        println!(
            "Configuration: {} connections, {} byte messages, {} seconds duration.",
            number, length, duration
            );
        println!();

        let effective_duration = duration;
        if effective_duration == 0 {
            println!("Duration was 0, cannot calculate per-second metrics.");
        } else {
            println!(
                "Throughput: {:.2} requests/sec, {:.2} responses/sec",
                total_sum.outb as f64 / effective_duration as f64,
                total_sum.inb as f64 / effective_duration as f64
                );
        }
        println!("Total Requests Sent: {}", total_sum.outb);
        println!("Total Responses Received: {}", total_sum.inb);
        if total_sum.outb > total_sum.inb {
            let lost_responses = total_sum.outb - total_sum.inb;
            println!(
                "Warning: {} responses were lost or not fully received ({}%).",
                lost_responses,
                if total_sum.outb > 0 { lost_responses as f64 * 100.0 / total_sum.outb as f64 } else { 0.0 }
                );
        } else if total_sum.outb < total_sum.inb {
            println!("Warning: More responses received than requests sent. This is unusual. ({} extra)", total_sum.inb - total_sum.outb);
        }

        println!("Benchmark finished.");
    }
