use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::io::AsRawFd;
use std::mem::MaybeUninit;
use std::time::{Duration, Instant};
use std::collections::HashSet;
use socket2::{Socket, Domain, Type, MsgHdrMut, MaybeUninitSlice};


use crate::packet::Packet; 
use crate::packet::NetworkData; 
use crate::packet::NetworkMetadata; 

pub struct NetworkClient {
    socket: UdpSocket,
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
    
    pub fn send_message(_target_addr: &str, payload_size: &usize, count: &Option<u64>,
                         pps: &Option<u64>) -> io::Result<()> {
                            
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;

        let addr: SocketAddr = _target_addr.parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        socket.connect(&addr.into())?;

        let mut max_buffer = [0u8; 65507]; 

        if *payload_size > max_buffer.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Payload size exceeds maximum buffer limit of 65507"));
        }
        let padding = &mut max_buffer[..*payload_size];

        for i in 0..1000 {
            let mut packet = Packet::new(i, padding);
            socket.send(&packet.to_bytes())?;
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

#[derive(Default)]
pub struct NetworkLatency {
    // Accpeting processing copy will cause packet delay
    pub latency             : Vec<f64>,
    pub avg_latency         : f64,
    pub max_latency         : f64,
    pub min_latency         : f64,
    pub p_50                : f64,
    pub p_99                : f64,
    pub p_99_9              : f64,
}

impl NetworkLatency {

   pub fn calc_latency(&mut self, later: &Duration, earlier: &Duration) -> Option<f64> {
        let ms = later.checked_sub(*earlier)?.as_secs_f64() * 1000.0;
        self.latency.push(ms);
        Some(ms)
    }

    fn quantile(&self, q: f64) -> f64 {
        let n = self.latency.len();
        let rank = ((q * n as f64).ceil() as usize).saturating_sub(1);
        self.latency[rank.min(n - 1)]
    }

    pub fn result(&mut self) {
        self.latency.sort_by(|a, b| a.total_cmp(b));

        let sum: f64 = self.latency.iter().sum();

        self.avg_latency = sum / self.latency.len() as f64;
        self.max_latency = self.latency[self.latency.len()-1];
        self.min_latency = self.latency[0];
        self.p_50   = self.quantile(0.50);
        self.p_99   = self.quantile(0.99);
        self.p_99_9 = self.quantile(0.999);
    }
}


pub struct NetworkInfo {
    pub next_id             : u64,
    pub expected_amount     : u64,  
    pub recv_out_of_order   : u64,
    pub pcr                 : f64,
    pub network_loss        : u64,
    pub kernal_loss         : u64,
    pub packet_count        : u64,

    pub network_latency     : NetworkLatency,
    pub total_latency       : NetworkLatency,

    pub process_time        : Duration,
    pub process_time_inst   : Instant,
    pub last_packet         : NetworkData,
    pub missing_ids         : HashSet<u64>,
    
}   


impl NetworkInfo {

    pub fn new() -> Self {

        let mut now: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut now) };
        
        let p_time = Duration::new(now.tv_sec as u64, now.tv_nsec as u32);
        Self {
            next_id             : 1,
            expected_amount     : 0,  
            recv_out_of_order   : 0,
            pcr                 : 0.0,
            network_loss        : 0,
            kernal_loss         : 0,
            packet_count        : 0,
            
            network_latency     : NetworkLatency::default(),
            total_latency       : NetworkLatency::default(),

            process_time        : p_time,
            process_time_inst   : Instant::now(),
            last_packet         : NetworkData::default(),
            missing_ids         : HashSet::new(),

        }
    }

    pub fn update_info(&mut self, _packet: &NetworkData) -> io::Result<()> {

        self.update_packet_tracking(&_packet)?;
        self.update_kernal_overlow(&_packet.metadata)?;

        self.calc_pcr();

        let net   = self.calc_network_latency(&_packet);
        let total = self.calc_total_latency(&_packet);

        println!(
            "[{:>8}] {:>6} B  {}  net {:>9}  total {:>9}  proc {:>7?}  ovfl {:>6}  miss {:>6}  pcr {:>7.3}%",
            _packet.packet.id,
            _packet.packet.packet_size,
            if _packet.metadata.is_hardware { "HW" } else { "SW" },
            net.map_or("--".to_string(),   |v| format!("{:.3} ms", v)),
            total.map_or("--".to_string(), |v| format!("{:.3} ms", v)),
            self.calc_process_latency(),
            self.kernal_loss,
            self.missing_ids.len(),
            self.pcr,
        );
        Ok(())
    }

    pub fn calc_total_latency(&mut self, _packet: &NetworkData) -> Option<f64>{        
        self.total_latency.calc_latency(&self.process_time, &_packet.packet.tx_time)
    }

    pub fn calc_network_latency(&mut self, _packet: &NetworkData) -> Option<f64>{        
        self.network_latency.calc_latency(&_packet.metadata.rx_time, &_packet.packet.tx_time)
    }

    pub fn calc_process_latency(&self) -> Duration {
        self.process_time_inst.elapsed()
    }

    pub fn calc_pcr(&mut self) {
        let count           = self.packet_count as f64;
        let missing_count   = self.missing_ids.len() as f64;
        self.pcr = count / (count +  missing_count) * 100.0;
    }

    pub fn start_process_time(&mut self) {
        self.process_time_inst = Instant::now();

        let mut now: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut now) };
        self.process_time = Duration::new(now.tv_sec as u64, now.tv_nsec as u32);
    }

    pub fn update_kernal_overlow(&mut self, _meta: &NetworkMetadata) -> io::Result<()> {
        self.kernal_loss = _meta.ovfl_count;
        Ok(())
    }

    fn update_packet_tracking(&mut self, _packet: &NetworkData) -> io::Result<()> {
        
        if self.last_packet.packet.id  > _packet.packet.id  {

            if self.missing_ids.contains(&_packet.packet.id) {
                self.missing_ids.remove(&_packet.packet.id);
                self.packet_count+=1;
            }

        } 
        else {

            if _packet.packet.id > self.next_id  {
                for i in self.last_packet.packet.id+1 .._packet.packet.id { self.missing_ids.insert(i); }
            }

            self.last_packet.packet.id  = _packet.packet.id;
            self.next_id = self.last_packet.packet.id + 1;
            self.packet_count+=1;
        }

        Ok(())
    }



    pub fn report(&mut self) {
        self.network_latency.result();
        self.total_latency.result();

        let bar = "─".repeat(58);
        println!("\n{bar}");
        println!(" packets received      {:>12}", self.packet_count);
        println!(" missing (end-to-end)  {:>12}", self.missing_ids.len());
        println!(" kernel drops (ovfl)   {:>12}", self.kernal_loss);
        println!(" PCR                   {:>12.3} %", self.pcr);
        println!("{bar}");
        println!(" latency (ms)  {:>14} {:>14}", "network", "total");
        for (label, a, b) in [
            ("min",   self.network_latency.min_latency, self.total_latency.min_latency),
            ("p50",   self.network_latency.p_50,        self.total_latency.p_50),
            ("p99",   self.network_latency.p_99,        self.total_latency.p_99),
            ("p99.9", self.network_latency.p_99_9,      self.total_latency.p_99_9),
            ("max",   self.network_latency.max_latency, self.total_latency.max_latency),
            ("avg",   self.network_latency.avg_latency, self.total_latency.avg_latency),
        ] {
            println!("   {:<10} {:>14.3} {:>14.3}", label, a, b);
        }
        println!("{bar}\n");
    }
}