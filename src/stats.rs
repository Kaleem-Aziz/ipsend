use std::time::{Duration, Instant};
use std::collections::HashSet;
use std::fmt::Write;

use crate::network_packet::NetworkData; 
use crate::network_packet::Metadata; 
use crate::output::RxSample; 

pub struct NetworkInfo {
    
    pub tracker                     : NetworkTracker,
    pub jitter                      : NetworkJitter,
    pub tx_to_nic                   : NetworkLatency,
    pub tx_to_kernal_rx_latency     : NetworkLatency,
    pub kernrx_to_pt                : NetworkLatency,

    pub recv_timestamp              : Duration,
    pub recv_instant                : Instant,
    
}   


impl NetworkInfo {

    pub fn new() -> Self {
        let mut now: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut now) };
        
        let p_time = Duration::new(now.tv_sec as u64, now.tv_nsec as u32);
        Self {

            tracker                     : NetworkTracker::default(),
            jitter                      : NetworkJitter ::default(),
            tx_to_nic                   : NetworkLatency::default(),
            tx_to_kernal_rx_latency     : NetworkLatency::default(),
            kernrx_to_pt                : NetworkLatency::default(),

            recv_timestamp      : p_time,
            recv_instant        : Instant::now(),
        }
    }

    pub fn update_info(&mut self, p: &NetworkData, app_rx: Duration) -> RxSample {

        let process     = self.recv_instant.elapsed();
        
        let tx_to_kern  = self.tx_to_kernal_rx_latency
                            .calculate_latency(&p.metadata.sw_timestamp.time, &p.header.timestamp.time);
        let kern_to_app = self.kernrx_to_pt
                            .calculate_latency(&app_rx, &p.metadata.sw_timestamp.time);
        let jitter      = self.jitter
                            .calculate(&p.header.timestamp.time, &p.metadata.sw_timestamp.time);

        let tx_to_nic = self.calculate_tx_to_nic(p);

        self.tracker.update_packet_tracking(p);
        
        RxSample {
            id: p.header.id,
            size: p.packet_size,
            hw: p.metadata.hw_timestamp.is_some(),
            tx_to_nic, tx_to_kern, kern_to_app, jitter, process,
            ovfl:         self.tracker.kernal_loss,
            network_loss: self.tracker.network_loss,
            missing:      self.tracker.missing_ids.len(),
            out_of_order: self.tracker.recv_out_of_order,
            pcr:          self.tracker.pcr,
        }
    }


    // TX to Hardware NIC / not going to work needs fixing
    fn calculate_tx_to_nic(&mut self, _packet: &NetworkData) -> Option<f64>{
    //     if let Some(hw_time) = &_packet.metadata.hw_timestamp {
    //         self.tx_to_nic.calculate_latency(&hw_time.time, &_packet.packet.tx_timestamp.time)
    //     } else {
            None
    //     }
    }

    pub fn start_process_time(&mut self) -> Duration {
        self.recv_instant = Instant::now();

        let mut now: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut now) };
        self.recv_timestamp = Duration::new(now.tv_sec as u64, now.tv_nsec as u32);
        self.recv_timestamp
    }

    pub fn report(&mut self) -> String {
        self.tx_to_nic.result();
        self.kernrx_to_pt.result();
        self.tx_to_kernal_rx_latency.result();

        let mut report = String::from("");
        let bar = "─".repeat(58);

        let _ = writeln!(report, "\n{}", bar); 
        let _ = writeln!(report, " packets received      {:>12}", self.tracker.packet_count);
        let _ = writeln!(report, " missing (end-to-end)  {:>12}", self.tracker.missing_ids.len());
        let _ = writeln!(report, " kernel drops (ovfl)   {:>12}", self.tracker.kernal_loss);
        let _ = writeln!(report, " Network drops         {:>12}", self.tracker.network_loss);
        let _ = writeln!(report, " PCR                   {:>12.3} %", self.tracker.pcr);
        let _ = writeln!(report, "{}", bar);
        let _ = writeln!(report, " latency (ms)  {:>14} {:>14} {:>14}", "TX->NIC", "TX->KernalRx", "KernalRx->Rcvmsg");

        for (label, a, b, c) in [
            ("min",   self.tx_to_nic.min_latency,    self.tx_to_kernal_rx_latency.min_latency,  self.kernrx_to_pt.min_latency),
            ("p50",   self.tx_to_nic.p_50,           self.tx_to_kernal_rx_latency.p_50,         self.kernrx_to_pt.p_50),
            ("p99",   self.tx_to_nic.p_99,           self.tx_to_kernal_rx_latency.p_99,         self.kernrx_to_pt.p_99),
            ("p99.9", self.tx_to_nic.p_99_9,         self.tx_to_kernal_rx_latency.p_99_9,       self.kernrx_to_pt.p_99_9),
            ("max",   self.tx_to_nic.max_latency,    self.tx_to_kernal_rx_latency.max_latency,  self.kernrx_to_pt.max_latency),
            ("avg",   self.tx_to_nic.avg_latency,    self.tx_to_kernal_rx_latency.avg_latency,  self.kernrx_to_pt.avg_latency),
        ] {
            let _ = writeln!(report, "   {:<10} {:>14.3} {:>14.3} {:>14.3}", label, a, b, c);
        }

        let _ = writeln!(report, "{}", bar);

        report
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

   pub fn calculate_latency(&mut self, new_time: &Duration, old_time: &Duration) -> Option<f64> {
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


#[derive(Default)]
pub struct NetworkTracker {
    pub next_id             : u64,
    pub expected_amount     : u64,  
    pub recv_out_of_order   : u64,
    pub pcr                 : f64,
    pub missing_ids         : HashSet<u64>,

    pub network_loss        : u64,
    pub kernal_loss         : u32,
    pub failed_decode       : u64,
    pub packet_count        : u64,

    last_packet             : u64,

}

impl NetworkTracker {

    fn update_packet_tracking(&mut self, p: &NetworkData) {
        if self.last_packet  > p.header.id  {
            if self.missing_ids.contains(&p.header.id) {
                self.missing_ids.remove(&p.header.id);
                self.packet_count+=1;
                self.recv_out_of_order+=1;
            }
        } 
        else {
            if p.header.id > self.next_id  {
                for i in self.last_packet+1 .. p.header.id { self.missing_ids.insert(i); }
            }

            self.last_packet  = p.header.id; 
            self.next_id = self.last_packet + 1;
            self.packet_count+=1;
        }

        self.calculate_pcr();
        self.update_kernal_overlow(&p.metadata);
        self.calculate_network_loss();
    }

    fn calculate_pcr(&mut self) {
        let count           = self.packet_count as f64;
        let missing_count   = self.missing_ids.len() as f64;
        self.pcr = count / (count +  missing_count) * 100.0;
    }

    fn calculate_network_loss(&mut self){
        self.network_loss = (self.missing_ids.len() as u64).saturating_sub(self.kernal_loss as u64);
    }

    fn update_kernal_overlow(&mut self, _meta: &Metadata) {
        self.kernal_loss = _meta.ovfl_count;
    }

    pub fn  add_failed_to_decode(&mut self) { self.failed_decode +=1; }
}

