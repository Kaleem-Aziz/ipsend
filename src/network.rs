use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::io::AsRawFd;
use std::mem::MaybeUninit;
use std::time::{Duration, Instant};
use std::collections::HashSet;
use socket2::{Socket, Domain, Type, MsgHdrMut, MaybeUninitSlice};
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};


use crate::packet::Packet; 
use crate::network_packet::NetworkData; 
use crate::network_packet::NetworkMetadata; 
use crate::stats::NetworkInfo;

pub struct NetworkClient {
    socket: UdpSocket,
}

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn handle_sigint(_: libc::c_int) {
    RUNNING.store(false, Ordering::Relaxed);
}

unsafe fn set_opt<T>(fd: i32, level: i32, name: i32, val: &T) -> io::Result<()> {
    if libc::setsockopt(fd, level, name,
                        val as *const _ as *const libc::c_void,
                        std::mem::size_of::<T>() as libc::socklen_t) != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}


impl NetworkClient {
    pub fn setup(_ip_addr: &str) -> io::Result<Self>{

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;
        socket.set_reuse_address(true)?;

        let addr: SocketAddr = _ip_addr.parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        
        println!(
                "Ip {} ",
                addr
            );

        socket.bind(&addr.into())?;

        let fd = socket.as_raw_fd();


        unsafe {
            let flags: libc::c_uint =
                libc::SOF_TIMESTAMPING_RX_SOFTWARE | // Kernel network stack ingress
                libc::SOF_TIMESTAMPING_RX_HARDWARE | // Physical NIC hardware ingress
                libc::SOF_TIMESTAMPING_RAW_HARDWARE|
                libc::SOF_TIMESTAMPING_SOFTWARE;     // Enable overall reporting framework
            
            // Enabling TIMSESTAMOING
            set_opt(fd, libc::SOL_SOCKET, libc::SO_TIMESTAMPING, &flags)?;

            // Enable RX KERNAL OVERFLOW COUNTER
            let on: libc::c_int = 1;
            set_opt(fd, libc::SOL_SOCKET, libc::SO_RXQ_OVFL, &on)?;
            
            libc::signal(libc::SIGINT, handle_sigint as libc::sighandler_t);
        }

        socket.set_read_timeout(Some(Duration::from_millis(500)))?;

        socket.set_recv_buffer_size(8 * 1024 * 1024)?;   // SO_RCVBUF socket queue size


        let std_socket: UdpSocket = socket.into();
        
        Ok(Self { socket: std_socket })

    }
    


    pub fn send_message(_target_addr: &str, payload_size: &usize, count: &i64, 
                        interval: &Duration) -> io::Result<()> {

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;

        let addr: SocketAddr = _target_addr.parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        socket.connect(&addr.into())?;

        let mut max_buffer = [0u8; 65507]; 

        let mut padding = &mut max_buffer[.. *payload_size];

        let mut i :i64 = 0;
        let mut next_send_time = Instant::now() + *interval;

        loop{
            
            let now = Instant::now();
            if now < next_send_time {
                thread::sleep(next_send_time - now);
            }

    
            let mut packet = Packet::new(i as u64, padding);
            socket.send(&packet.to_bytes())?;

            i +=1;
            next_send_time += *interval;

            if i == *count {break};

        }

        Ok(())
        
    }

    pub fn start_listening (&self) -> io::Result<()> {

        let mut network_info = NetworkInfo::new();

        let socket_ref = socket2::SockRef::from(&self.socket);
        

        const MAX_PAYLOAD  :usize = 65507;
        const HEADER_SIZE  :usize = 24;
        const PAYLOAD_SZIE :usize = 65507-24;

        // Creating Buffers for kernal to push data into 
        let mut header      = [MaybeUninit::<u8>::uninit(); HEADER_SIZE];
        let mut buf         = [MaybeUninit::<u8>::uninit(); PAYLOAD_SZIE];
        

        // Aligning to 8 Bytes so CMSG_ Pointer correctly works
        #[repr(align(8))]
        struct ControlBuf([MaybeUninit<u8>; 1024]);
        let mut control_buf = ControlBuf([MaybeUninit::uninit(); 1024]);


        println!("Waiting for data...");
        
        loop {
            if !RUNNING.load(Ordering::Relaxed) { 
                network_info.report();    
                break; }

            let mut iov = [
                MaybeUninitSlice::new(&mut header), // Header Datagram
                MaybeUninitSlice::new(&mut buf),    // payload datagram
                ]; // I/O vector - direct memory pointer 

            let mut msg = MsgHdrMut::new()
                .with_buffers(&mut iov)
                .with_control(&mut control_buf.0); // create memory structure to metadata
                            
            // Take one datagram at a time and push into our buffers
            let amt = match socket_ref.recvmsg(&mut msg, 0) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };


            if amt < HEADER_SIZE {
                eprintln!("runt packet: {amt} bytes");
                continue;
            }

            
            let payload_len = amt - HEADER_SIZE;
            let control_len = msg.control_len();

            network_info.start_process_time();
            

            // Fat pointer to our  memory 
            let hdr     = unsafe { std::slice::from_raw_parts(header.as_ptr() as *const u8, HEADER_SIZE) };
            let payload = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, payload_len) };
            let control = unsafe { std::slice::from_raw_parts(control_buf.0.as_ptr() as *const u8, control_len) };
    
            let packet = NetworkData::to_packet(hdr, payload, control)?;
            network_info.update_info(&packet)?;

        }

        Ok(())
    }
    
    
}
