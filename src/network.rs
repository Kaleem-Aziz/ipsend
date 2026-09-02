use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::io::AsRawFd;
use std::mem::MaybeUninit;
use socket2::{Socket, Domain, Type, SockRef, MsgHdrMut, MaybeUninitSlice, RecvFlags};

use crate::packet::Packet; 


pub struct NetworkClient {
    socket: UdpSocket,
}

impl NetworkClient {
    pub fn setup(ip_addr: &str) -> io::Result<Self>{

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;
        socket.set_reuse_address(true)?;

        let addr: SocketAddr = ip_addr.parse()
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
                libc::SOF_TIMESTAMPING_TX_SOFTWARE | // Kernel network stack egress
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

        let socket_ref = socket2::SockRef::from(&self.socket);
        
        let mut buf         = [MaybeUninit::<u8>::uninit(); 1024];
        let mut control_buf = [MaybeUninit::<u8>::uninit(); 1024];

        println!("Waiting for data...");
        
        loop {
            let mut iov = [MaybeUninitSlice::new(&mut buf)]; // I/O vector - direct memory pointer 
            let mut msg = MsgHdrMut::new()
                .with_buffers(&mut iov)
                .with_control(&mut control_buf); // create memory structure to hold data + headers
            

            let start = std::time::Instant::now();

            let amt = socket_ref.recvmsg(&mut msg, 0)?;
            
            let received_at = std::time::Instant::now();

            let received = unsafe {         // Grab all written bits
                std::slice::from_raw_parts(
                    buf.as_ptr() as *const u8,
                    amt,
                )
            };

            let packet = Packet::to_packet(received)?; 
                    
            let elapsed = received_at.elapsed();
            println!(
                "Received {} bytes. id={}, processing took {:?}",
                amt,
                packet.id,
                elapsed
            );

        }

        Ok(())
    }
    
    pub fn send_message(target_addr: &str) -> io::Result<()> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;

        let addr: SocketAddr = target_addr.parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        socket.connect(&addr.into())?;

        for i in 1..10 {
            let packet = Packet::create_packet(i*10);
            socket.send(&packet.to_bytes())?;
        }

        Ok(())
        
    }
    
}
