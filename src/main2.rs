use std::env;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::thread;
use std::time::{Duration, Instant};

fn print_usage(program: &str) {
    println!(
        r#"Echo benchmark.

Usage:
  {program} [ -a <address> ] [ -l <length> ] [ -c <number> ] [ -t <duration> ]
"#,
        program = program
    );
}

struct SharedCount {
    inb: Arc<AtomicU64>,
    outb: Arc<AtomicU64>,
}

fn main() {
    // 기본값 설정
    let args: Vec<String> = env::args().collect();
    let mut address = "127.0.0.1:12345".to_string();
    let mut length = 512;
    let mut duration = 60;
    let mut number = 50;

    // 인자 파싱
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-a" => {
                i += 1;
                if i < args.len() {
                    address = args[i].clone();
                }
            }
            "-l" => {
                i += 1;
                if i < args.len() {
                    length = args[i].parse().unwrap_or(512);
                }
            }
            "-t" => {
                i += 1;
                if i < args.len() {
                    duration = args[i].parse().unwrap_or(60);
                }
            }
            "-c" => {
                i += 1;
                if i < args.len() {
                    number = args[i].parse().unwrap_or(50);
                }
            }
            "-h" | "--help" => {
                print_usage(&args[0]);
                return;
            }
            _ => {}
        }
        i += 1;
    }

    // 공유 자원 초기화
    let shared = Arc::new(SharedCount {
        inb: Arc::new(AtomicU64::new(0)),
        outb: Arc::new(AtomicU64::new(0)),
    });

    let stop = Arc::new(AtomicBool::new(false));
    let stop_weak = Arc::downgrade(&stop);

    // 클라이언트 스레드 시작
     for _ in 0..number {
         let shared = Arc::clone(&shared);
         let stop = Arc::clone(&stop);
         let address = address.clone();

         thread::spawn(move || {
             let mut out_buf: Vec<u8> = vec![0; length];
             out_buf[length - 1] = b'\n';
             let mut in_buf: Vec<u8> = vec![0; length];
             let mut stream = TcpStream::connect(&*address).unwrap();
             
             while !stop.load(Ordering::Relaxed) {
                 if stream.write_all(&out_buf).is_err() {
                     break;
                 }
                 shared.outb.fetch_add(1, Ordering::Relaxed);

                 if stream.read_exact(&mut in_buf).is_err() {
                     break;
                 }
                 shared.inb.fetch_add(1, Ordering::Relaxed);
             }
         });
     }

     // 중간 통계 출력 스레드
     {
         let shared = Arc::clone(&shared);
         thread::spawn(move || {
             for elapsed in 1..=duration {
                 thread::sleep(Duration::from_secs(1));
                 if elapsed % 60 == 0 {
                     let inb = shared.inb.load(Ordering::Relaxed);
                     let outb = shared.outb.load(Ordering::Relaxed);
                     println!(
                         "[{}s] Partial stats: sent={}, received={}",
                         elapsed, outb, inb
                     );
                 }
             }
         });
     }

     // 전체 실행 시간 대기
     thread::sleep(Duration::from_secs(duration));

     // 스레드 중단 신호
     if let Some(s) = Weak::upgrade(&stop_weak) {
         s.store(true, Ordering::Relaxed);
     }

     // 약간의 여유 시간
     thread::sleep(Duration::from_secs(2));

     // 최종 결과 출력
     let inb = shared.inb.load(Ordering::Relaxed);
     let outb = shared.outb.load(Ordering::Relaxed);
     println!();
     println!("Benchmarking: {}", address);
     println!(
         "{} clients, running {} bytes, {} sec.",
         number, length, duration
         );
     println!();
     println!(
         "Speed: {} request/sec, {} response/sec",
         outb / duration as u64,
         inb / duration as u64
         );
     println!("Requests: {}", outb);
     println!("Responses: {}", inb);
}
