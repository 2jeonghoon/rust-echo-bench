use getopts::Options;
use chrono::Local; // chrono 크레이트 필요
use std::{
		collections::HashMap,
				fs::{OpenOptions, create_dir_all},
				io::{BufWriter, Write},
				net::UdpSocket,
				sync::{Arc, Mutex, mpsc, atomic::{Ordering, AtomicBool}},
				thread,
				env,
				time::{Duration, Instant},
};
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
	let stop_rx= Arc::new(AtomicBool::new(false));
	let stop_tx= Arc::new(AtomicBool::new(false));

	println!(
		"Starting UDP benchmark with {} connections, {} byte messages, for {} seconds, to {}",
		number, length, duration, address
	);

	let mut handles = Vec::new();
	
	// let now_str = Local::now().format("%Y%m%d_%H%M%S").to_string();
	let run_id = format!("{}", chrono::Local::now().format("%Y%m%d_%H%M%S_%6f"));


	for id in 0..number {
		let tx_clone = tx.clone();
		let stop_clone = Arc::clone(&stop_tx);
		let stop_rx_clone = Arc::clone(&stop_rx);
		let address_clone = address.clone();

		let dir_path = format!("latency/{}", run_id);

		create_dir_all(&dir_path).expect("Failed to create directory");
		
		let log_path = format!("{}/thread_{}.txt", dir_path, id);
		handles.push(thread::spawn(move || {
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

			socket.set_read_timeout(Some(Duration::from_secs(10))).ok();
	
			let sent_map: Arc<Mutex<HashMap<String, Instant>>> = Arc::new(Mutex::new(HashMap::new()));
			let sent_for_rx = Arc::clone(&sent_map);
			let sent_for_tx = Arc::clone(&sent_map);
	
			let socket_rx = socket.try_clone().expect("Socket clone failed");

			let inb_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
			let inb_counter_clone = Arc::clone(&inb_counter);
			
			let mut outb: u64 = 0;
			let mut rng = rand::thread_rng();
			let mut buf = vec![0u8; length];
			let send_interval = Duration::from_millis(1000);

			// assign
			let assign_msg: String = (0..length - 1).map(|_| rng.sample(rand::distributions::Alphanumeric) as char).collect();
				
			buf[..length - 1].copy_from_slice(assign_msg.as_bytes());
			buf[length - 1] = b'\n';

			if socket.send(&buf).is_ok() {
					outb += 0;
			}
			
			thread::sleep(Duration::from_millis(100));


			let rx_handle = thread::spawn(move || {
				let mut in_buf = vec![0u8; length];
				let file = OpenOptions::new().create_new(true).append(true).open(&log_path).expect("open latency log failed");
				let mut writer = BufWriter::new(file);

				while !stop_rx_clone.load(Ordering::Relaxed) {
					match socket_rx.recv(&mut in_buf) {
						Ok(received) => {
							let received_msg = String::from_utf8_lossy(&in_buf[..received]);
							// println!("recv: {}", received_msg);

							if let Some(sent_time) = {
								let mut m = sent_for_rx.lock().unwrap();
								m.remove(received_msg.as_ref())
							} {
								// println!("match: {}", received_msg);
								let latency_ms = sent_time.elapsed().as_secs_f64() * 1000.0;
								if let Err(e) = writeln!(writer, "{}", latency_ms) {
									eprintln!("[thread {id}] write failed: {e}");
								}

							}	

							inb_counter_clone.fetch_add(1, Ordering::Relaxed);
						}	
						
						Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
							let now = Local::now();
							println!("[{}] recv timeout 발생", now.format("%Y-%m-%d %H:%M:%S"));
							break;
						}
						Err(e) => {
							eprintln!("recv 에러: {}", e);
						}
					}
				}

				// thread::sleep(Duration::from_secs(300));
		
				if let Err(e) = writer.flush() {
					eprintln!("[thread {id}] final flush failed: {e}");
				}
				
				println!("recv thread 종료");
			});

			while !stop_clone.load(Ordering::Relaxed) {
				let msg: String = (0..length - 1).map(|_| rng.sample(rand::distributions::Alphanumeric) as char).collect();
				let msg_line = format!("{msg}\n");
				// println!("send: {}", msg);
				
				buf[..length - 1].copy_from_slice(msg.as_bytes());
				buf[length - 1] = b'\n';

				{
					let mut m = sent_for_tx.lock().unwrap();
					m.insert(msg_line.clone(), Instant::now());
				}

				if socket.send(&buf).is_ok() {
					outb += 1;
				}

				thread::sleep(send_interval);
			}

			let inb = inb_counter.load(Ordering::Relaxed);
			let _ = tx_clone.send(Count {inb, outb});

			println!("send thread join()");
			rx_handle.join().expect("join failed");

			{        
				let map = sent_map.lock().unwrap();
				println!("sent_map 길이: {}", map.len());  
			}

			println!("send thread 종료");
		}));
	}

	thread::sleep(Duration::from_secs(duration));
	stop_tx.store(true, Ordering::Relaxed);
	// thread::sleep(Duration::from_secs(300));
	// stop_rx.store(true, Ordering::Relaxed);

	for h in handles { h.join().unwrap(); }

	let mut total = Count { inb: 0, outb: 0 };
	for _ in 0..number {
		if let Ok(c) = rx.recv_timeout(Duration::from_secs(5)) {
			total.inb += c.inb;
			total.outb += c.outb;
		}
	}

		println!("Total Sent: {}, Total Received: {}", total.outb, total.inb);
}
