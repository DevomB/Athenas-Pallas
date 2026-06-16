//! Download historical OHLCV from Binance or Yahoo Finance.

use athenas_pallas::data::fetch::binance;
use athenas_pallas::data::fetch::intervals::{interval_hint, normalize_interval, FetchProvider};
use athenas_pallas::data::fetch::yahoo;
use clap::Parser;
use std::path::PathBuf;
use time::{Duration, OffsetDateTime};

#[derive(Parser, Debug)]
#[command(name = "pallas-fetch")]
struct Args {
    #[arg(long, value_parser = ["binance", "yahoo"])]
    provider: String,
    #[arg(long)]
    symbol: String,
    #[arg(long, default_value = "1d")]
    interval: String,
    #[arg(long)]
    start: Option<String>,
    #[arg(long)]
    end: Option<String>,
    #[arg(long)]
    days: Option<u64>,
    #[arg(long, default_value = "1y")]
    range: String,
    #[arg(short, long)]
    output: PathBuf,
    /// Print documented intervals for the selected provider and exit.
    #[arg(long)]
    list_intervals: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let provider = FetchProvider::parse(&args.provider)
        .ok_or_else(|| format!("unknown provider: {}", args.provider))?;
    if args.list_intervals {
        for iv in provider.documented_intervals() {
            println!("{iv}");
        }
        return Ok(());
    }
    let interval = normalize_interval(&args.interval);
    if let Some(hint) = interval_hint(provider, &interval) {
        eprintln!("note: {hint}");
    }
    let client = reqwest::Client::new();
    match provider {
        FetchProvider::Binance => {
            let end = args
                .end
                .as_deref()
                .map(parse_date)
                .transpose()?
                .unwrap_or_else(OffsetDateTime::now_utc);
            let start = if let Some(s) = &args.start {
                parse_date(s)?
            } else {
                let days = args.days.unwrap_or(365);
                end - Duration::days(days as i64)
            };
            binance::fetch_klines_csv(
                &client,
                &args.symbol.to_uppercase(),
                &interval,
                start.unix_timestamp() * 1000,
                end.unix_timestamp() * 1000,
                &args.output,
            )
            .await?;
        }
        FetchProvider::Yahoo => {
            let range = if args.days.is_some() {
                format!("{}d", args.days.unwrap())
            } else {
                args.range.clone()
            };
            yahoo::fetch_chart_csv(&client, &args.symbol, &interval, &range, &args.output).await?;
        }
    }
    println!("wrote {}", args.output.display());
    Ok(())
}

fn parse_date(s: &str) -> Result<OffsetDateTime, Box<dyn std::error::Error>> {
    let format = time::format_description::parse("[year]-[month]-[day]")?;
    let date = time::Date::parse(s, &format)?;
    Ok(date.midnight().assume_utc())
}
