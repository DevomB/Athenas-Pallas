# Style

- No phase/gate/milestone language in identifiers, comments, commits, or filenames.
- Normal names: `BarSeries`, `run_backtest`, `ExternalStrategy`.
- Comments only where logic is non-obvious.
- Hot path: sync, dense indices, preloaded bars. Cold path: JSON IPC, CSV load once.
