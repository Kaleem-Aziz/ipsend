use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ipsend", version, about = "Packet Generation / Measurement tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Mode,
}

#[derive(Subcommand)]
pub enum Mode {
    /// Listen for packets and report latency and loss
    Recv {
        /// Address to bind, e.g. 0.0.0.0:9000
        #[arg(long, short)]
        addr: String,

        /// Write per-packet samples to a CSV file
        #[arg(long)]
        csv: Option<PathBuf>,

        /// Print a line for every packet
        #[arg(short, long)]
        verbose: bool,
    },

    /// Send test packets
    #[command(group(
            clap::ArgGroup::new("rate")
                .args(["pps", "bps", "interval"])
                .required(true) ))]
    Send {
        /// Destination, e.g. 10.0.0.5:9000
        #[arg(long, short)]
        addr: String,

        /// Payload size in bytes
        #[arg(short = 'b', long = "bytes", default_value_t = 576)] // minimum range 24 / Max Range 65507
        size: usize,

        /// Seconds between packets 
        #[arg(short = 'i' )]
        interval: Option<f64>,

        /// Number of packets to send
        #[arg(short = 'c', long = "count", default_value_t=-1)]
        count: i64,

        /// Rate limit in packets per second 
        #[arg(long = "pps")]
        pps: Option<u64>,

        /// How many bits per second to send
        #[arg(long = "bps")]
        bps: Option<u64>,
    },
}