//! Subprocess strategy adapter (Python, C++, etc.).

use std::io::{BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::events::{Event, OrderIntent};
use crate::instrument::InstrumentMeta;
use crate::strategy::protocol::{
    intents_to_orders, snapshot_from, EventMsg, InitMsg, InstrumentInfo, IntentsMsg, ReadyMsg,
    ShutdownMsg,
};
use crate::strategy::{Strategy, StrategyContext};
use crate::types::InstrumentId;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs a strategy in a child process over newline-delimited JSON.
pub struct ExternalStrategy {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    lines: mpsc::Receiver<String>,
    seq: u64,
    instrument: InstrumentId,
    protocol_error: Option<Error>,
}

impl ExternalStrategy {
    /// Spawn a Python script.
    pub fn spawn_python(script: &std::path::Path, python: &str) -> Result<Self> {
        let child = Command::new(python)
            .arg(script)
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
            protocol_error: None,
        })
    }

    /// Init handshake with instrument metadata and balances.
    pub fn handshake(
        &mut self,
        instrument: InstrumentId,
        meta: &InstrumentMeta,
        balances: &std::collections::HashMap<crate::types::Asset, rust_decimal::Decimal>,
        fee_bps: rust_decimal::Decimal,
    ) -> Result<()> {
        self.instrument = instrument.clone();
        let mut bal = std::collections::HashMap::new();
        for (a, v) in balances {
            bal.insert(a.0.to_string(), v.to_string());
        }
        let init = InitMsg {
            msg: "init".into(),
            instruments: vec![InstrumentInfo {
                exchange: instrument.exchange.clone(),
                symbol: instrument.symbol.clone(),
                base: meta.base.0.to_string(),
                quote: meta.quote.0.to_string(),
            }],
            balances: bal,
            config: [("fee_bps".into(), fee_bps.to_string())]
                .into_iter()
                .collect(),
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
            ctx: snapshot_from(ctx.state, &self.instrument),
        };
        let line = serde_json::to_string(&msg)
            .map_err(|e| Error::StrategyProtocol(format!("serialize event: {e}")))?;
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
        intents_to_orders(parsed.intents)
    }
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
