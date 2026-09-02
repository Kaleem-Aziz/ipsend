use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::io::AsRawFd;
use std::mem::MaybeUninit;
use std::time::Instant;
use socket2::{Socket, Domain, Type, MsgHdrMut, MaybeUninitSlice};

use crate::packet::Packet; 
use crate::packet::NetworkPacket; 

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
    pub process_time        : Instant,
    pub last_packet         : NetworkPacket,
    pub missing_ids         : Vec<u64>,
    
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
                libc::SOF_TIMESTAMPING_SOFTWARE;     // Enable overall reporting framework

            if libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_TIMESTAMPING,
                &flags as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_uint>() as libc::socklen_t,
            ) != 0 {
                return Err(io::Error::last_os_error());
            }
        }

        let std_socket: UdpSocket = socket.into();
        
        Ok(Self { socket: std_socket })

    }
    
    pub fn start_listening (&self) -> io::Result<()> {

        let mut network_info = NetworkInfo::new();

        let socket_ref = socket2::SockRef::from(&self.socket);
        
        let mut buf         = [MaybeUninit::<u8>::uninit(); 1024];
        let mut control_buf = [MaybeUninit::<u8>::uninit(); 1024];

        println!("Waiting for data...");
        
        loop {
            let mut iov = [MaybeUninitSlice::new(&mut buf)]; // I/O vector - direct memory pointer 
            let mut msg = MsgHdrMut::new()
                .with_buffers(&mut iov)
                .with_control(&mut control_buf); // create memory structure to metadata
            

            let amt = socket_ref.recvmsg(&mut msg, 0)?;


            network_info.start_time();

            let received = unsafe {         // Grab all written bits
                std::slice::from_raw_parts(
                    buf.as_ptr() as *const u8,
                    amt,
                )
            };

            
            let packet = Packet::to_packet(received, &control_buf)?; 
            
            network_info.update_info(&packet)?;

        }

        Ok(())
    }
    
    pub fn send_message(_target_addr: &str) -> io::Result<()> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;

        let addr: SocketAddr = _target_addr.parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        socket.connect(&addr.into())?;

        for i in 1..10 {
            let packet = Packet::create_packet(i*10);
            socket.send(&packet.to_bytes())?;
        }

        Ok(())
        
    }
    
}


impl NetworkInfo {

    pub fn new() -> Self {
        Self {
            next_id             : 0,
            expected_amount     : 0,  
            recv_out_of_order   : 0,
            pcr                 : 0,
            avg_latency         : 0,
            max_latency         : 0,
            min_latency         : 0,
            process_time        : Instant::now(),
            last_packet         : NetworkPacket::default(),
            missing_ids         : Vec::new(),

        }
    }

    pub fn update_info(&mut self, _packet: &NetworkPacket) -> io::Result<()> {

        self.update_packet_tracking(&_packet)?;
        let elapsed = self.process_time.elapsed();

            println!(
                "Received  bytes. id={}, processing took {:?} Transmit time  \n",
                // packet.packet.,
                _packet.packet.id,
                elapsed,
            );

        Ok(())
    }


    pub fn start_time(&mut self) {
        self.process_time = Instant::now();
    }

    fn update_packet_tracking(&mut self, _packet: &NetworkPacket) -> io::Result<()> {
        
        if self.last_packet.packet.id  >= _packet.packet.id  {
            println!("Old Packet");
        } else {

            if self.next_id != _packet.packet.id {
                println!("Jumped Ahead");
                for i in self.last_packet.packet.id .._packet.packet.id { self.missing_ids.push(i); }
            }

            self.last_packet.packet.id  = _packet.packet.id;
            self.next_id = self.last_packet.packet.id  + 10;
        }

        Ok(())
    }
}