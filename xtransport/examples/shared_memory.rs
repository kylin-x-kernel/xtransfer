use shared_memory::ShmemConf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use xtransport::{Read, Result, TransportConfig, Write, XTransport};

const BUFFER_SIZE: usize = 2048; // 2KB shared memory buffer
const DATA_SIZE: usize = 10 * 1024 * 1024; // 10MB test data

/// Shared memory stream wrapper that implements Read/Write traits
struct SharedMemoryStream {
    ptr: *mut u8,
    size: usize,
    read_pos: Arc<AtomicUsize>,
    write_pos: Arc<AtomicUsize>,
    closed: Arc<AtomicBool>,
}

unsafe impl Send for SharedMemoryStream {}
unsafe impl Sync for SharedMemoryStream {}

impl SharedMemoryStream {
    fn new_writer(ptr: *mut u8, size: usize, read_pos: Arc<AtomicUsize>, write_pos: Arc<AtomicUsize>, closed: Arc<AtomicBool>) -> Self {
        Self {
            ptr,
            size,
            read_pos,
            write_pos,
            closed,
        }
    }

    fn new_reader(ptr: *mut u8, size: usize, read_pos: Arc<AtomicUsize>, write_pos: Arc<AtomicUsize>, closed: Arc<AtomicBool>) -> Self {
        Self {
            ptr,
            size,
            read_pos,
            write_pos,
            closed,
        }
    }

    fn available_read(&self) -> usize {
        let write_pos = self.write_pos.load(Ordering::Acquire);
        let read_pos = self.read_pos.load(Ordering::Acquire);
        if write_pos >= read_pos {
            write_pos - read_pos
        } else {
            0
        }
    }

    fn available_write(&self) -> usize {
        let write_pos = self.write_pos.load(Ordering::Acquire);
        let read_pos = self.read_pos.load(Ordering::Acquire);
        let used = write_pos - read_pos;
        BUFFER_SIZE.saturating_sub(used)
    }
}

impl Read for SharedMemoryStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Wait for data to be available
        loop {
            let available = self.available_read();
            if available > 0 {
                break;
            }
            if self.closed.load(Ordering::Acquire) {
                return Ok(0); // EOF
            }
            thread::sleep(Duration::from_micros(10));
        }

        let available = self.available_read();
        let to_read = buf.len().min(available);
        let read_pos = self.read_pos.load(Ordering::Acquire);
        let offset = read_pos % BUFFER_SIZE;

        unsafe {
            let src = self.ptr.add(offset);
            let remaining = self.size - offset;
            
            if to_read <= remaining {
                std::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), to_read);
            } else {
                // Wrap around
                std::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), remaining);
                let src2 = self.ptr;
                std::ptr::copy_nonoverlapping(src2, buf.as_mut_ptr().add(remaining), to_read - remaining);
            }
        }

        self.read_pos.fetch_add(to_read, Ordering::Release);
        Ok(to_read)
    }
}

impl Write for SharedMemoryStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Wait for space to be available
        loop {
            let available = self.available_write();
            if available > 0 {
                break;
            }
            if self.closed.load(Ordering::Acquire) {
                return Err(xtransport::Error::new(xtransport::error::ErrorKind::Other));
            }
            thread::sleep(Duration::from_micros(10));
        }

        let available = self.available_write();
        let to_write = buf.len().min(available);
        let write_pos = self.write_pos.load(Ordering::Acquire);
        let offset = write_pos % BUFFER_SIZE;

        unsafe {
            let dst = self.ptr.add(offset);
            let remaining = self.size - offset;
            
            if to_write <= remaining {
                std::ptr::copy_nonoverlapping(buf.as_ptr(), dst, to_write);
            } else {
                // Wrap around
                std::ptr::copy_nonoverlapping(buf.as_ptr(), dst, remaining);
                let dst2 = self.ptr;
                std::ptr::copy_nonoverlapping(buf.as_ptr().add(remaining), dst2, to_write - remaining);
            }
        }

        self.write_pos.fetch_add(to_write, Ordering::Release);
        Ok(to_write)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

fn main() {
    env_logger::init();

    println!("Creating shared memory...");
    let shmem = match ShmemConf::new()
        .size(BUFFER_SIZE)
        .create()
    {
        Ok(m) => Arc::new(m),
        Err(e) => {
            eprintln!("Failed to create shared memory: {}", e);
            return;
        }
    };

    let read_pos = Arc::new(AtomicUsize::new(0));
    let write_pos = Arc::new(AtomicUsize::new(0));
    let closed = Arc::new(AtomicBool::new(false));

    println!("Starting writer thread...");
    let writer_ptr = shmem.as_ptr() as usize;  // Convert to usize for Send
    let writer_read_pos = read_pos.clone();
    let writer_write_pos = write_pos.clone();
    let writer_closed = closed.clone();
    
    let writer_handle = thread::spawn(move || {
        let stream = SharedMemoryStream::new_writer(
            writer_ptr as *mut u8,  // Convert back to pointer
            BUFFER_SIZE,
            writer_read_pos,
            writer_write_pos,
            writer_closed.clone(),
        );
        let mut transport = XTransport::new(stream, TransportConfig::default());

        println!("[Writer] Sending {} MB of data...", DATA_SIZE / 1024 / 1024);
        let data = vec![0x42u8; DATA_SIZE];
        
        let start = std::time::Instant::now();
        match transport.send_message(&data) {
            Ok(_) => {
                let elapsed = start.elapsed();
                let speed = DATA_SIZE as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;
                println!("[Writer] Sent {} MB in {:.2}s, Speed: {:.2} MB/s",
                    DATA_SIZE / 1024 / 1024,
                    elapsed.as_secs_f64(),
                    speed
                );
            }
            Err(e) => {
                eprintln!("[Writer] Failed to send: {:?}", e);
            }
        }

        writer_closed.store(true, Ordering::Release);
        println!("[Writer] Done");
    });

    println!("Starting reader thread...");
    let reader_ptr = shmem.as_ptr() as usize;  // Convert to usize for Send
    let reader_read_pos = read_pos.clone();
    let reader_write_pos = write_pos.clone();
    let reader_closed = closed.clone();
    
    let reader_handle = thread::spawn(move || {
        // Give writer a head start
        thread::sleep(Duration::from_millis(100));

        let stream = SharedMemoryStream::new_reader(
            reader_ptr as *mut u8,  // Convert back to pointer
            BUFFER_SIZE,
            reader_read_pos,
            reader_write_pos,
            reader_closed,
        );
        let mut transport = XTransport::new(stream, TransportConfig::default());

        println!("[Reader] Receiving data...");
        let start = std::time::Instant::now();
        
        match transport.recv_message() {
            Ok(data) => {
                let elapsed = start.elapsed();
                let speed = data.len() as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;
                println!("[Reader] Received {} MB in {:.2}s, Speed: {:.2} MB/s",
                    data.len() / 1024 / 1024,
                    elapsed.as_secs_f64(),
                    speed
                );

                // Verify data
                if data.iter().all(|&b| b == 0x42) {
                    println!("[Reader] Data verification: PASSED ✓");
                } else {
                    println!("[Reader] Data verification: FAILED ✗");
                }
            }
            Err(e) => {
                eprintln!("[Reader] Failed to receive: {:?}", e);
            }
        }

        println!("[Reader] Done");
    });

    println!("Waiting for threads to complete...");
    writer_handle.join().unwrap();
    reader_handle.join().unwrap();

    println!("\n=== Shared Memory Example Complete ===");
    println!("This example demonstrates using shared memory as a transport");
    println!("for the xtransport protocol, with automatic fragmentation and");
    println!("reassembly of large messages.");
}
