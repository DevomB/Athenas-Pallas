//! Download historical OHLCV from Alpha Vantage.

use athenas_pallas::data::fetch::alpha_vantage::{
    fetch_daily, filter_date_range, AlphaVantageAsset,
};
use athenas_pallas::data::fetch::write_ohlcv_csv;
use athenas_pallas::data::fetch::intervals::{interval_hint, normalize_interval, FetchProvider};
use clap::Parser;
use std::path::PathBuf;
use time::OffsetDateTime;

#[derive(Parser, Debug)]
#[command(name = "pallas-fetch")]
struct Args {
    #[arg(long, default_value = "alpha-vantage", value_parser = ["alpha-vantage", "alphavantage", "alpha", "av"])]
    provider: String,
    /// Asset family for Alpha Vantage daily bars.
    #[arg(long, default_value = "equity", value_parser = ["equity", "stock", "stocks", "etf", "fund", "crypto", "cryptocurrency", "digital-currency"])]
    asset: String,
    #[arg(long)]
    symbol: String,
    /// Quote market for Alpha Vantage crypto daily bars.
    #[arg(long, default_value = "USD")]
    market: String,
    #[arg(long, default_value = "1d")]
    interval: String,
    #[arg(long)]
    start: Option<String>,
    #[arg(long)]
    end: Option<String>,
    #[arg(long)]
    days: Option<u64>,
    /// Alpha Vantage output size. Free keys currently support compact.
    #[arg(long, default_value = "compact", value_parser = ["compact", "full"])]
    outputsize: String,
    /// Alpha Vantage API key. Prefer ALPHA_VANTAGE_API_KEY or local .env over this flag.
    #[arg(long)]
    api_key: Option<String>,
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
        return Err(hint.into());
    }
    let client = reqwest::Client::new();
    match provider {
        FetchProvider::AlphaVantage => {
            let api_key = args.api_key.or_else(load_alpha_vantage_key).ok_or_else(|| {
                "missing Alpha Vantage key; set ALPHA_VANTAGE_API_KEY or add it to local .env"
                    .to_string()
            })?;
            let asset = AlphaVantageAsset::parse(&args.asset)
                .ok_or_else(|| format!("unknown Alpha Vantage asset: {}", args.asset))?;
            let end = args.end.as_deref().map(parse_date).transpose()?;
            let start = args.start.as_deref().map(parse_date).transpose()?;
            let symbol = normalize_alpha_vantage_symbol(asset, &args.symbol, &args.market);
            let mut bars = fetch_daily(
                &client,
                &api_key,
                asset,
                &symbol,
                &args.market.to_uppercase(),
                &args.outputsize,
            )
            .await?;
            let start = if start.is_none() {
                args.days
                    .and_then(|days| bars.len().checked_sub(days as usize))
                    .and_then(|ix| bars.get(ix).map(|bar| bar.ts))
            } else {
                start
            };
            bars = filter_date_range(bars, start, end);
            write_ohlcv_csv(&args.output, &bars)?;
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

fn load_alpha_vantage_key() -> Option<String> {
    std::env::var("ALPHA_VANTAGE_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(read_dotenv_alpha_vantage_key)
}

fn read_dotenv_alpha_vantage_key() -> Option<String> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let path = dir.join(".env");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            for line in contents.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                if key.trim() == "ALPHA_VANTAGE_API_KEY" {
                    let value = value.trim().trim_matches('"').trim_matches('\'');
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn normalize_alpha_vantage_symbol(
    asset: AlphaVantageAsset,
    symbol: &str,
    market: &str,
) -> String {
    let mut symbol = symbol
        .trim()
        .replace(['-', '/', '_'], "")
        .to_ascii_uppercase();
    if asset == AlphaVantageAsset::Crypto {
        let market = market.trim().to_ascii_uppercase();
        let suffixes = [market.as_str(), "USDT", "USDC", "USD"];
        for suffix in suffixes {
            if !suffix.is_empty() && symbol.len() > suffix.len() && symbol.ends_with(suffix) {
                symbol.truncate(symbol.len() - suffix.len());
                break;
            }
        }
    }
    symbol
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_crypto_pairs_for_alpha_vantage() {
        assert_eq!(
            normalize_alpha_vantage_symbol(AlphaVantageAsset::Crypto, "BTCUSDT", "USD"),
            "BTC"
        );
        assert_eq!(
            normalize_alpha_vantage_symbol(AlphaVantageAsset::Crypto, "ETH-USD", "USD"),
            "ETH"
        );
    }

    #[test]
    fn leaves_equity_tickers_intact() {
        assert_eq!(
            normalize_alpha_vantage_symbol(AlphaVantageAsset::Equity, "BRK.B", "USD"),
            "BRK.B"
        );
    }
}
