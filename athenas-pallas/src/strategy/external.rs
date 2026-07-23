//! Subprocess strategy adapter (Python, C++, etc.).

use std::ffi::OsString;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::events::{Event, OrderIntent};
use crate::instrument::InstrumentMeta;
use crate::strategy::protocol::{
    intents_to_orders, snapshot_from, EventMsg, FinishMsg, InitMsg, InstrumentInfo, IntentsMsg,
    ReadyMsg, ShutdownMsg,
};
use crate::strategy::{Strategy, StrategyContext, StrategyControl};
use crate::types::{ClientOrderId, InstrumentId};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs a strategy in a child process over newline-delimited JSON.
pub struct ExternalStrategy {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    lines: mpsc::Receiver<String>,
    seq: u64,
    instrument: InstrumentId,
    controls: Vec<StrategyControl>,
    fills_seen: usize,
    rejections_seen: usize,
    supports_finish: bool,
    diagnostics: serde_json::Map<String, serde_json::Value>,
    protocol_error: Option<Error>,
}

impl ExternalStrategy {
    /// Spawn a Python script.
    pub fn spawn_python(script: &Path, python: &str) -> Result<Self> {
        let mut command = Command::new(python);
        command.arg(script);
        if let Some(python_path) = strategy_python_path(script)? {
            command.env("PYTHONPATH", python_path);
        }
        let child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(Error::Io)?;
        Self::from_child(child)
    }

    /// Spawn a compiled strategy binary.
    pub fn spawn_binary(path: &std::path::Path) -> Result<Self> {
        let binary = std::fs::canonicalize(path).map_err(Error::Io)?;
        let child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(Error::Io)?;
        Self::from_child(child)
    }

    fn from_child(mut child: Child) -> Result<Self> {
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Invalid("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Invalid("no stdout".into()))?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || reader_loop(stdout, tx));
        Ok(Self {
            child,
            stdin: BufWriter::new(stdin),
            lines: rx,
            seq: 0,
            instrument: InstrumentId::new("", ""),
            controls: Vec::new(),
            fills_seen: 0,
            rejections_seen: 0,
            supports_finish: false,
            diagnostics: serde_json::Map::new(),
            protocol_error: None,
        })
    }

    /// Initialize a strategy with one instrument and no custom parameters.
    pub fn handshake(
        &mut self,
        instrument: InstrumentId,
        meta: &InstrumentMeta,
        balances: &std::collections::HashMap<crate::types::Asset, rust_decimal::Decimal>,
        fee_bps: rust_decimal::Decimal,
    ) -> Result<()> {
        let instruments = std::collections::HashMap::from([(instrument.clone(), meta.clone())]);
        self.handshake_with_context(
            instrument,
            &instruments,
            balances,
            fee_bps,
            &std::collections::HashMap::new(),
        )
    }

    /// Versioned handshake with complete instrument context and arbitrary strategy parameters.
    pub fn handshake_with_context(
        &mut self,
        instrument: InstrumentId,
        instruments: &std::collections::HashMap<InstrumentId, InstrumentMeta>,
        balances: &std::collections::HashMap<crate::types::Asset, rust_decimal::Decimal>,
        fee_bps: rust_decimal::Decimal,
        parameters: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        self.instrument = instrument.clone();
        let mut bal = std::collections::HashMap::new();
        for (a, v) in balances {
            bal.insert(a.0.to_string(), v.to_string());
        }
        let mut instrument_info: Vec<_> = instruments
            .iter()
            .map(|(instrument, meta)| InstrumentInfo {
                exchange: instrument.exchange.clone(),
                symbol: instrument.symbol.clone(),
                base: meta.base.0.to_string(),
                quote: meta.quote.0.to_string(),
                asset_class: format!("{:?}", meta.asset_class).to_ascii_lowercase(),
                lot_size: meta.lot_size.map(|value| value.to_string()),
                tick_size: meta.tick_size.map(|value| value.to_string()),
                contract_multiplier: meta.contract_multiplier.map(|value| value.to_string()),
                expiry: meta.expiry.clone(),
                margin_initial_rate: meta.margin_initial_rate.map(|value| value.to_string()),
                option_kind: meta
                    .option_kind
                    .map(|kind| format!("{kind:?}").to_ascii_lowercase()),
                option_exercise_style: meta
                    .option_exercise_style
                    .map(|style| format!("{style:?}").to_ascii_lowercase()),
                option_strike: meta.option_strike.map(|value| value.to_string()),
                option_underlying: meta.option_underlying.clone(),
            })
            .collect();
        instrument_info.sort_by(|left, right| {
            (&left.exchange, &left.symbol).cmp(&(&right.exchange, &right.symbol))
        });
        let init = InitMsg {
            msg: "init".into(),
            protocol_version: 2,
            instruments: instrument_info,
            balances: bal,
            config: [("fee_bps".into(), fee_bps.to_string())]
                .into_iter()
                .collect(),
            parameters: parameters.clone(),
        };
        let line = serde_json::to_string(&init)?;
        writeln!(self.stdin, "{line}").map_err(Error::Io)?;
        self.stdin.flush().map_err(Error::Io)?;
        self.read_ready()
    }

    /// Return protocol error from the last event, if any.
    pub fn take_error(&mut self) -> Result<()> {
        if let Some(e) = self.protocol_error.take() {
            return Err(e);
        }
        Ok(())
    }

    fn fail(&mut self, err: Error) {
        self.protocol_error = Some(err);
    }

    fn read_ready(&mut self) -> Result<()> {
        let line = self.read_line_timeout(HANDSHAKE_TIMEOUT)?;
        let ready: ReadyMsg = serde_json::from_str(&line)
            .map_err(|e| Error::StrategyProtocol(format!("expected ready: {e}; line={line}")))?;
        if ready.msg != "ready" {
            return Err(Error::StrategyProtocol(format!(
                "expected ready, got {}",
                ready.msg
            )));
        }
        self.supports_finish = ready
            .capabilities
            .iter()
            .any(|capability| capability == "finish");
        Ok(())
    }

    fn read_line_timeout(&mut self, timeout: Duration) -> Result<String> {
        if let Some(status) = self.child.try_wait().map_err(Error::Io)? {
            return Err(Error::StrategyProtocol(format!(
                "strategy process exited: {status}"
            )));
        }
        match self.lines.recv_timeout(timeout) {
            Ok(line) if line.trim().is_empty() => {
                Err(Error::StrategyProtocol("empty response".into()))
            }
            Ok(line) => Ok(line),
            Err(RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                Err(Error::StrategyProtocol("handshake timeout".into()))
            }
            Err(RecvTimeoutError::Disconnected) => {
                Err(Error::StrategyProtocol("strategy process exited".into()))
            }
        }
    }

    fn orders_for_event(
        &mut self,
        ctx: &StrategyContext,
        event: &Event,
    ) -> Result<Vec<OrderIntent>> {
        self.seq += 1;
        let seq = self.seq;
        let msg = EventMsg {
            msg: "event".into(),
            seq,
            event: event.clone(),
            ctx: snapshot_from(
                ctx.state,
                &self.instrument,
                self.fills_seen,
                self.rejections_seen,
            ),
        };
        let line = serde_json::to_string(&msg)
            .map_err(|e| Error::StrategyProtocol(format!("serialize event: {e}")))?;
        self.request_orders(ctx, seq, line)
    }

    fn orders_for_finish(&mut self, ctx: &StrategyContext) -> Result<Vec<OrderIntent>> {
        if !self.supports_finish {
            return Ok(Vec::new());
        }
        self.seq += 1;
        let seq = self.seq;
        let msg = FinishMsg {
            msg: "finish".into(),
            seq,
            ctx: snapshot_from(
                ctx.state,
                &self.instrument,
                self.fills_seen,
                self.rejections_seen,
            ),
        };
        let line = serde_json::to_string(&msg)
            .map_err(|error| Error::StrategyProtocol(format!("serialize finish: {error}")))?;
        self.request_orders(ctx, seq, line)
    }

    fn request_orders(
        &mut self,
        ctx: &StrategyContext,
        seq: u64,
        line: String,
    ) -> Result<Vec<OrderIntent>> {
        writeln!(self.stdin, "{line}")
            .and_then(|_| self.stdin.flush())
            .map_err(|_| Error::StrategyProtocol("write to strategy failed".into()))?;
        let line = self.read_line_timeout(Duration::from_secs(30))?;
        let parsed: IntentsMsg = serde_json::from_str(&line)
            .map_err(|e| Error::StrategyProtocol(format!("parse intents: {e}; line={line}")))?;
        if parsed.msg != "intents" {
            return Err(Error::StrategyProtocol(format!(
                "expected intents, got {}",
                parsed.msg
            )));
        }
        if parsed.seq != seq {
            return Err(Error::StrategyProtocol(format!(
                "seq mismatch expected {seq} got {}",
                parsed.seq
            )));
        }
        self.fills_seen = ctx.state.fill_log.len();
        self.rejections_seen = ctx.state.rejection_log.len();
        self.controls.extend(
            parsed
                .cancel_order_ids
                .iter()
                .cloned()
                .map(StrategyControl::CancelOrder),
        );
        self.controls.extend(
            parsed
                .cancel_client_order_ids
                .iter()
                .cloned()
                .map(|id| StrategyControl::CancelClientOrder(ClientOrderId(id))),
        );
        if parsed.cancel_all {
            self.controls.push(StrategyControl::CancelAll);
        }
        if parsed.flatten {
            self.controls.push(StrategyControl::Flatten);
        }
        self.diagnostics.extend(parsed.diagnostics);
        intents_to_orders(parsed.intents)
    }
}

fn strategy_python_path(script: &Path) -> Result<Option<OsString>> {
    let script = std::fs::canonicalize(script).map_err(Error::Io)?;
    let mut paths: Vec<PathBuf> = std::env::var_os("PYTHONPATH")
        .as_deref()
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect();
    let mut roots = Vec::new();
    if let Some(strategy_dir) = script.parent() {
        paths.push(strategy_dir.to_path_buf());
        if let Some(parent) = strategy_dir.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.extend(cwd.ancestors().map(Path::to_path_buf));
    }
    for root in roots {
        for candidate in [
            root.join("_common/python"),
            root.join("_sdk/python"),
            root.join("trading/_common/python"),
            root.join("trading/_sdk/python"),
            root.join("trading-algos/_common/python"),
        ] {
            if candidate.is_dir() {
                paths.push(candidate);
            }
        }
    }
    paths.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
    paths.dedup_by(|left, right| {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    });
    if paths.is_empty() {
        return Ok(None);
    }
    std::env::join_paths(paths)
        .map(Some)
        .map_err(|error| Error::Invalid(format!("invalid Python search path: {error}")))
}

fn reader_loop(stdout: ChildStdout, tx: mpsc::Sender<String>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = String::new();
        match std::io::BufRead::read_line(&mut reader, &mut line) {
            Ok(0) => break,
            Ok(_) => {
                if tx.send(line).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

impl Strategy for ExternalStrategy {
    fn on_event(&mut self, ctx: &StrategyContext, event: &Event, out: &mut Vec<OrderIntent>) {
        if self.protocol_error.is_some() {
            return;
        }
        match self.orders_for_event(ctx, event) {
            Ok(orders) => out.extend(orders),
            Err(e) => self.fail(e),
        }
    }

    fn drain_controls(&mut self, out: &mut Vec<StrategyControl>) {
        out.append(&mut self.controls);
    }

    fn on_finish(&mut self, ctx: &StrategyContext<'_>, out: &mut Vec<OrderIntent>) {
        if self.protocol_error.is_some() {
            return;
        }
        match self.orders_for_finish(ctx) {
            Ok(orders) => out.extend(orders),
            Err(error) => self.fail(error),
        }
    }

    fn diagnostics(&self) -> serde_json::Map<String, serde_json::Value> {
        self.diagnostics.clone()
    }
}

impl Drop for ExternalStrategy {
    fn drop(&mut self) {
        let _ = writeln!(
            self.stdin,
            "{}",
            serde_json::to_string(&ShutdownMsg {
                msg: "shutdown".into(),
            })
            .unwrap_or_default()
        );
        let _ = self.stdin.flush();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_path_includes_strategy_common_and_sdk() {
        let root =
            std::env::temp_dir().join(format!("pallas-python-path-{}", uuid::Uuid::new_v4()));
        let strategy = root.join("trading/example");
        let common = root.join("trading/_common/python");
        let sdk = root.join("trading/_sdk/python");
        std::fs::create_dir_all(&strategy).unwrap();
        std::fs::create_dir_all(&common).unwrap();
        std::fs::create_dir_all(&sdk).unwrap();
        let script = strategy.join("strategy.py");
        std::fs::write(&script, "").unwrap();

        let joined = strategy_python_path(&script).unwrap().unwrap();
        let paths: Vec<_> = std::env::split_paths(&joined).collect();
        assert!(paths.contains(&std::fs::canonicalize(&strategy).unwrap()));
        assert!(paths.contains(&std::fs::canonicalize(&common).unwrap()));
        assert!(paths.contains(&std::fs::canonicalize(&sdk).unwrap()));

        std::fs::remove_dir_all(root).unwrap();
    }
}
