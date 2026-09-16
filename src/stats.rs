use std::io;
use std::time::{Duration, Instant};
use std::collections::HashSet;


use crate::packet::Packet; 
use crate::network_packet::NetworkData; 
use crate::network_packet::NetworkMetadata; 



pub struct NetworkInfo {
    pub next_id             : u64,
    pub expected_amount     : u64,  
    pub recv_out_of_order   : u64,
    pub pcr                 : f64,
    pub missing_ids         : HashSet<u64>,
    
    pub network_loss        : u64,
    pub kernal_loss         : u32,
    pub packet_count        : u64,


    pub jitter                      : NetworkJitter,
    pub tx_to_nic                   : NetworkLatency,
    pub tx_to_kernal_rx_latency     : NetworkLatency,
    pub kernrx_to_pt                : NetworkLatency,

    pub recv_timestamp      : Duration,
    pub recv_instant        : Instant,
    pub last_packet         : NetworkData,
    
    
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
            missing_ids         : HashSet::new(),


            network_loss        : 0,
            kernal_loss         : 0,
            packet_count        : 0,

            jitter                      : NetworkJitter::default(),
            tx_to_nic                   : NetworkLatency::default(),
            tx_to_kernal_rx_latency     : NetworkLatency::default(),
            kernrx_to_pt                : NetworkLatency::default(),

            recv_timestamp      : p_time,
            recv_instant        : Instant::now(),
            last_packet         : NetworkData::default(),
            

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
             "[{:>8}] {:>6} B  TX->NIC {:>9}  TX->KernalRx {:>9}  KernalRx->Rcvmsg {:>9}  Rcvmsg->Processed {:>7?}  jitter {:>6} ovfl {:>6}  miss {:>6}  out_of_order {:>5} pcr {:>7.3}%",
            _packet.packet.id,
            _packet.packet.packet_size,
            net.map_or("--".to_string(),   |v| format!("{:.3} ms", v)),
            tx_to_kernal_rx_latency.map_or("--".to_string(), |v| format!("{:.3} ms", v)),
            kernrx_to_pt.map_or("--".to_string(), |v| format!("{:.3} ms", v)),
            process_t,
            jitter.map_or("--".to_string(), |v| format!("{:.3} ms", v)),
            self.kernal_loss,
            self.missing_ids.len(),
            self.recv_out_of_order,
            self.pcr,
        );

        
        self.update_packet_tracking(&_packet)?;

        Ok(())
    }


    // Tx to sk-buff latency
    fn calc_tx_to_kernal_rx_latency(&mut self, _packet: &NetworkData) -> Option<f64>{        
        self.tx_to_kernal_rx_latency.calc_latency(&_packet.metadata.sw_timestamp.time, &_packet.packet.tx_timestamp.time)
    }


    // TX to Hardware NIC / not going to work needs fixing
    fn calc_tx_to_nic(&mut self, _packet: &NetworkData) -> Option<f64>{      
    //     if let Some(hw_time) = &_packet.metadata.hw_timestamp {  
    //         self.tx_to_nic.calc_latency(&hw_time.time, &_packet.packet.tx_timestamp.time)
    //     } else {
            None
    //     }
    }


    // Latency from kernal sk_buff to proccessing of data
    fn calc_kernal_rx_to_process_latency(&mut self, _packet: &NetworkData) -> Option<f64> {
        self.kernrx_to_pt.calc_latency(&self.recv_timestamp, &_packet.metadata.sw_timestamp.time)

    }

    fn calc_jitter(&mut self, p: &NetworkData) -> Option<f64> {
        self.jitter.calculate(&p.packet.tx_timestamp.time, &p.metadata.sw_timestamp.time)
    }
    

    fn calc_process_latency(&self) -> Duration {
        self.recv_instant.elapsed()
    }

    fn calc_pcr(&mut self) {
        let count           = self.packet_count as f64;
        let missing_count   = self.missing_ids.len() as f64;
        self.pcr = count / (count +  missing_count) * 100.0;
    }


    fn update_kernal_overlow(&mut self, _meta: &NetworkMetadata) -> io::Result<()> {
        self.kernal_loss = _meta.ovfl_count;
        Ok(())
    }

    fn update_packet_tracking(&mut self, _packet: &NetworkData) -> io::Result<()> {
        
        if self.last_packet.packet.id  > _packet.packet.id  {
            if self.missing_ids.contains(&_packet.packet.id) {
                self.missing_ids.remove(&_packet.packet.id);
                self.packet_count+=1;
                self.recv_out_of_order+=1;
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

    pub fn start_process_time(&mut self) {
        self.recv_instant = Instant::now();

        let mut now: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut now) };
        self.recv_timestamp = Duration::new(now.tv_sec as u64, now.tv_nsec as u32);
    }

    pub fn report(&mut self) {
        self.tx_to_nic.result();
        self.kernrx_to_pt.result();
        self.tx_to_kernal_rx_latency.result();

        let bar = "─".repeat(58);
        println!("\n{bar}");
        println!(" packets received      {:>12}", self.packet_count);
        println!(" missing (end-to-end)  {:>12}", self.missing_ids.len());
        println!(" kernel drops (ovfl)   {:>12}", self.kernal_loss);
        println!(" PCR                   {:>12.3} %", self.pcr);
        println!("{bar}");
        println!(" latency (ms)  {:>14} {:>14} {:>14}", "TX->NIC", "TX->KernalRx", "KernalRx->Rcvmsg");
        for (label, a, b, c) in [
            ("min",   self.tx_to_nic.min_latency,    self.tx_to_kernal_rx_latency.min_latency,  self.kernrx_to_pt.min_latency, ),
            ("p50",   self.tx_to_nic.p_50,           self.tx_to_kernal_rx_latency.p_50,         self.kernrx_to_pt.p_50,        ),
            ("p99",   self.tx_to_nic.p_99,           self.tx_to_kernal_rx_latency.p_99,         self.kernrx_to_pt.p_99,        ),
            ("p99.9", self.tx_to_nic.p_99_9,         self.tx_to_kernal_rx_latency.p_99_9,       self.kernrx_to_pt.p_99_9,      ),
            ("max",   self.tx_to_nic.max_latency,    self.tx_to_kernal_rx_latency.max_latency,  self.kernrx_to_pt.max_latency, ),
            ("avg",   self.tx_to_nic.avg_latency,    self.tx_to_kernal_rx_latency.avg_latency,  self.kernrx_to_pt.avg_latency, ),
        ] {
            println!("   {:<10} {:>14.3} {:>14.3} {:>14.3}", label, a, b, c);
        }
        println!("{bar}\n");
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
        if self.latency.is_empty() { return; }
        self.latency.sort_by(|a, b| a.total_cmp(b));

        let sum: f64 = self.latency.iter().sum();

        self.avg_latency = sum / self.latency.len() as f64;


        self.max_latency = self.latency[ self.latency.len().saturating_sub(1) ];
        self.min_latency = self.latency[0];
        self.p_50   = self.quantile(0.50);
        self.p_99   = self.quantile(0.99);
        self.p_99_9 = self.quantile(0.999);
    }
}



#[derive(Default)]
pub struct NetworkJitter {
    last_transit: Option<f64>,
    jitter_ms: f64,

    
}

impl NetworkJitter {
    /// Returns None until two samples exist or if clocks run backwards.
    pub fn calculate(&mut self, tx: &Duration, rx: &Duration) -> Option<f64> {

        let transit = rx.checked_sub(*tx)?.as_secs_f64();
        let last = self.last_transit.replace(transit)?;
        let d = (transit - last).abs() * 1000.0;

        self.jitter_ms += (d - self.jitter_ms) / 16.0;

        Some(self.jitter_ms)
    }
}
