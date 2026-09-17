use std::time::Duration;

pub struct Sample {
    pub id           : u64,
    pub size         : u64,
    pub hw           : bool,          // was a hardware timestamp present?
    pub tx_to_nic    : Option<f64>,   // ms
    pub tx_to_kern   : Option<f64>,   // ms
    pub kern_to_app  : Option<f64>,
    pub jitter       : Option<f64>,
    pub process      : Duration,
    pub network_loss : u64,
    pub ovfl         : u32,
    pub missing      : usize,
    pub out_of_order : u64,
    pub pcr          : f64,
}

impl Sample {
    pub const CSV_HEADER: &'static str =
        "id,size,hw,tx_to_nic_ms,tx_to_kern_ms,kern_to_app_ms,jitter_ms,\
         process_ns,ovfl,network_loss,missing,out_of_order,pcr";

    pub fn csv_row(&self) -> String {
        fn f(v: Option<f64>) -> String {
            v.map_or(String::new(), |x| format!("{x:.6}"))
        }
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{:.4}",
            self.id,
            self.size,
            if self.hw { 1 } else { 0 },
            f(self.tx_to_nic),
            f(self.tx_to_kern),
            f(self.kern_to_app),
            f(self.jitter),
            self.process.as_nanos(),
            self.ovfl,
            self.network_loss,
            self.missing,
            self.out_of_order,
            self.pcr,
        )
    }

    pub fn line(&self) -> String {
        fn f(v: Option<f64>) -> String {
            v.map_or("--".to_string(), |x| format!("{x:.3} ms"))
        }
        format!(
            "[{:>8}] {:>6} B {}  TX->NIC {:>9}  TX->KernalRx {:>9}  KernalRx->Rcvmsg {:>9}  \
             Rcvmsg->Processed {:>7?}  jitter {:>9} ovfl {:>6} network loss {:>6} miss {:>6}  \
             out_of_order {:>5} pcr {:>7.3}%",
            self.id,
            self.size,
            if self.hw { "HW" } else { "SW" },
            f(self.tx_to_nic),
            f(self.tx_to_kern),
            f(self.kern_to_app),
            self.process,
            f(self.jitter),
            self.ovfl,
            self.network_loss,
            self.missing,
            self.out_of_order,
            self.pcr,
        )
    }
}