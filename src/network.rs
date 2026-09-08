use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::io::AsRawFd;
use std::mem::MaybeUninit;
use std::time::Instant;
use std::collections::HashSet;
use socket2::{Socket, Domain, Type, MsgHdrMut, MaybeUninitSlice};


use crate::packet::Packet; 
use crate::packet::NetworkData; 
use crate::packet::NetworkMetadata; 

pub struct NetworkClient {
    socket: UdpSocket,
}


pub struct NetworkInfo {
    pub next_id             : u64,
    pub expected_amount     : u64,  
    pub recv_out_of_order   : u64,
    pub pcr                 : u64,
    pub avg_latency         : u64,
    pub max_latency         : u64,
    pub min_latency         : u64,
    pub network_loss        : u64,
    pub kernal_loss         : u64,
    pub process_time        : Instant,
    pub last_packet         : NetworkData,
    pub missing_ids         : HashSet<u64>,
    
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
        }

        socket.set_recv_buffer_size(8 * 1024 * 1024)?;   // SO_RCVBUF socket queue size


        let std_socket: UdpSocket = socket.into();
        
        Ok(Self { socket: std_socket })

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
            
            let mut iov = [
                MaybeUninitSlice::new(&mut header), // Header Datagram
                MaybeUninitSlice::new(&mut buf),    // payload datagram
                ]; // I/O vector - direct memory pointer 

            let mut msg = MsgHdrMut::new()
                .with_buffers(&mut iov)
                .with_control(&mut control_buf.0); // create memory structure to metadata
            
            // Take one datagram at a time and push into our buffers
            let amt = socket_ref.recvmsg(&mut msg, 0)?;

            if amt < HEADER_SIZE {
                eprintln!("runt packet: {amt} bytes");
                continue;
            }

            
            let payload_len = amt - HEADER_SIZE;
            let control_len = msg.control_len();

            network_info.start_time();
            

            // Fat pointer to our  memory 
            let hdr     = unsafe { std::slice::from_raw_parts(header.as_ptr() as *const u8, HEADER_SIZE) };
            let payload = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, payload_len) };
            let control = unsafe { std::slice::from_raw_parts(control_buf.0.as_ptr() as *const u8, control_len) };
    
            let packet = NetworkData::to_packet(hdr, payload, control)?;
            network_info.update_info(&packet)?;

        }

        Ok(())
    }
    
    pub fn send_message(_target_addr: &str, payload_size: &usize) -> io::Result<()> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;

        let addr: SocketAddr = _target_addr.parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        socket.connect(&addr.into())?;

        let mut max_buffer = [0u8; 65507]; 

        if *payload_size > max_buffer.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Payload size exceeds maximum buffer limit"));
        }
        let padding = &mut max_buffer[..*payload_size];

        for i in 0..1000 {
            let mut packet = Packet::new(i, padding);
            socket.send(&packet.to_bytes())?;
        }

        Ok(())
        
    }
    
}


impl NetworkInfo {

    pub fn new() -> Self {
        Self {
            next_id             : 1,
            expected_amount     : 0,  
            recv_out_of_order   : 0,
            pcr                 : 0,
            avg_latency         : 0,
            network_loss        : 0,
            kernal_loss         : 0,
            max_latency         : u64::MIN,
            min_latency         : u64::MAX,
            process_time        : Instant::now(),
            last_packet         : NetworkData::default(),
            missing_ids         : HashSet::new(),

        }
    }

    pub fn update_info(&mut self, _packet: &NetworkData) -> io::Result<()> {

        self.update_packet_tracking(&_packet)?;
        self.update_kernal_overlow(&_packet.metadata)?;

        let elapsed = self.process_time.elapsed();

            println!(
                "Received  bytes {} id={}, processing took {:?} Transmit time  \nCaptured TS (HW={}): {}.{:09}s \n Ovfl {} Missing Pkts Count {}",
                _packet.packet.packet_size,
                _packet.packet.id,
                elapsed,
                _packet.metadata.is_hardware, 
                _packet.metadata.sec, 
                _packet.metadata.nsec,
                _packet.metadata.ovfl_count,
                self.missing_ids.len()
            );

        Ok(())
    }


    pub fn start_time(&mut self) {
        self.process_time = Instant::now();
    }

    pub fn update_kernal_overlow(&mut self, _meta: &NetworkMetadata) -> io::Result<()> {
        self.kernal_loss = _meta.ovfl_count;
        Ok(())
    }

    fn update_packet_tracking(&mut self, _packet: &NetworkData) -> io::Result<()> {
        
        if self.last_packet.packet.id  > _packet.packet.id  {
            println!("Old Packet old {}  rx {}" ,self.last_packet.packet.id, _packet.packet.id );

            if self.missing_ids.contains(&_packet.packet.id) {
                self.missing_ids.remove(&_packet.packet.id);
            }

        } 
        else {

            if _packet.packet.id > self.next_id  {
                println!("Jumped Ahead");
                for i in self.last_packet.packet.id .._packet.packet.id+1 { self.missing_ids.insert(i); }
            }

            self.last_packet.packet.id  = _packet.packet.id;
            self.next_id = self.last_packet.packet.id + 1;
        }

        Ok(())
    }
}