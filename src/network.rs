use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::io::AsRawFd;
use std::mem::MaybeUninit;
use std::time::{Duration, Instant};
use std::collections::HashSet;
use socket2::{Socket, Domain, Type, MsgHdrMut, MaybeUninitSlice};
use std::thread;

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
    


    pub fn send_message(_target_addr: &str, payload_size: &usize, count: &i64, 
                        interval: &Duration) -> io::Result<()> {

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;

        let addr: SocketAddr = _target_addr.parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        socket.connect(&addr.into())?;

        let mut max_buffer = [0u8; 65507]; 

        if *payload_size > 65507 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Payload size exceeds maximum buffer limit of 65507"));
        }
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

   pub fn calc_latency(&mut self, new_time: &Duration, old_time: &Duration) -> Option<f64> {
        let ms = new_time.checked_sub(*old_time)?.as_secs_f64() * 1000.0;
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
    pub kernal_loss         : u32,
    pub packet_count        : u64,
    pub jitter              : f64,

    pub tx_to_nic                   : NetworkLatency,
    pub tx_to_kernal_rx_latency     : NetworkLatency,
    pub kernrx_to_pt                : NetworkLatency,

    pub recv_timestamp      : Duration,
    pub recv_instant        : Instant,
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
            jitter              : 0.0,
            
            tx_to_nic                   : NetworkLatency::default(),
            tx_to_kernal_rx_latency     : NetworkLatency::default(),
            kernrx_to_pt                : NetworkLatency::default(),

            recv_timestamp      : p_time,
            recv_instant        : Instant::now(),
            last_packet         : NetworkData::default(),
            missing_ids         : HashSet::new(),

        }
    }

    pub fn update_info(&mut self, _packet: &NetworkData) -> io::Result<()> {

        let process_t = self.calc_process_latency();

        self.update_kernal_overlow(&_packet.metadata)?;

        self.calc_pcr();

        let net   = self.calc_tx_to_nic(&_packet);
        let tx_to_kernal_rx_latency = self.calc_tx_to_kernal_rx_latency(&_packet);
        let kernrx_to_pt = self.calc_kernal_rx_to_process_latency(&_packet);
        let jitter = self.calc_jitter(&_packet);
        
        

        println!(
             "[{:>8}] {:>6} B  TX->NIC {:>9}  TX->KernalRx {:>9}  KernalRx->Rcvmsg {:>9}  Rcvmsg->Processed {:>7?}  jitter {:>6.3} ovfl {:>6}  miss {:>6}  pcr {:>7.3}%",
            _packet.packet.id,
            _packet.packet.packet_size,
            net.map_or("--".to_string(),   |v| format!("{:.3} ms", v)),
            tx_to_kernal_rx_latency.map_or("--".to_string(), |v| format!("{:.3} ms", v)),
            kernrx_to_pt.map_or("--".to_string(), |v| format!("{:.3} ms", v)),
            process_t,
            jitter,
            self.kernal_loss,
            self.missing_ids.len(),
            self.pcr,
        );

        
        self.update_packet_tracking(&_packet)?;

        Ok(())
    }


    // Tx to sk-buff latency
    pub fn calc_tx_to_kernal_rx_latency(&mut self, _packet: &NetworkData) -> Option<f64>{        
        self.tx_to_kernal_rx_latency.calc_latency(&_packet.metadata.sw_timestamp.time, &_packet.packet.tx_timestamp.time)
    }


    // TX to Hardware NIC / not going to work needs fixing
    pub fn calc_tx_to_nic(&mut self, _packet: &NetworkData) -> Option<f64>{      
        if let Some(hw_time) = &_packet.metadata.hw_timestamp {  
            self.tx_to_nic.calc_latency(&hw_time.time, &_packet.packet.tx_timestamp.time)
        } else {
            None
        }
    }


    // Latency from kernal sk_buff to proccessing of data
    pub fn calc_kernal_rx_to_process_latency(&mut self, _packet: &NetworkData) -> Option<f64> {
        self.kernrx_to_pt.calc_latency(&self.recv_timestamp, &_packet.metadata.sw_timestamp.time)

    }


    pub fn calc_jitter(&mut self, _packet: &NetworkData) -> f64 {

        let tx = _packet.packet.tx_timestamp.time;
        let rx = _packet.metadata.sw_timestamp.time;
        let delta = rx.checked_sub(tx).expect("REASON").as_secs_f64();

        let last_tx = self.last_packet.packet.tx_timestamp.time;
        let last_rx = self.last_packet.metadata.sw_timestamp.time;
        let last_delta = last_rx.checked_sub(last_tx).expect("REASON").as_secs_f64();


        let d = ((delta - last_delta).abs()) * 1000.0;     
        self.jitter += (d - self.jitter) / 16.0;            // RFC 3550 smoothing
        self.jitter

    }

    pub fn calc_process_latency(&self) -> Duration {
        self.recv_instant.elapsed()
    }

    pub fn calc_pcr(&mut self) {
        let count           = self.packet_count as f64;
        let missing_count   = self.missing_ids.len() as f64;
        self.pcr = count / (count +  missing_count) * 100.0;
    }

    pub fn start_process_time(&mut self) {
        self.recv_instant = Instant::now();

        let mut now: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut now) };
        self.recv_timestamp = Duration::new(now.tv_sec as u64, now.tv_nsec as u32);
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

            self.last_packet  = *_packet; // Using Copy function
            self.next_id = self.last_packet.packet.id + 1;
            self.packet_count+=1;
        }

        Ok(())
    }


    pub fn report(&mut self) {
        self.tx_to_nic.result();
        self.tx_to_kernal_rx_latency.result();

        let bar = "─".repeat(58);
        println!("\n{bar}");
        println!(" packets received      {:>12}", self.packet_count);
        println!(" missing (end-to-end)  {:>12}", self.missing_ids.len());
        println!(" kernel drops (ovfl)   {:>12}", self.kernal_loss);
        println!(" PCR                   {:>12.3} %", self.pcr);
        println!("{bar}");
        println!(" latency (ms)  {:>14} {:>14}", "TX->KernalRx", "KernalRx->proc");
        for (label, a, b) in [
            ("min",   self.tx_to_nic.min_latency, self.tx_to_kernal_rx_latency.min_latency),
            ("p50",   self.tx_to_nic.p_50,        self.tx_to_kernal_rx_latency.p_50),
            ("p99",   self.tx_to_nic.p_99,        self.tx_to_kernal_rx_latency.p_99),
            ("p99.9", self.tx_to_nic.p_99_9,      self.tx_to_kernal_rx_latency.p_99_9),
            ("max",   self.tx_to_nic.max_latency, self.tx_to_kernal_rx_latency.max_latency),
            ("avg",   self.tx_to_nic.avg_latency, self.tx_to_kernal_rx_latency.avg_latency),
        ] {
            println!("   {:<10} {:>14.3} {:>14.3}", label, a, b);
        }
        println!("{bar}\n");
    }
}