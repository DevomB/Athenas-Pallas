const ERROR_PATTERNS: Array<{ pattern: RegExp; message: string }> = [
  {
    pattern: /credentials not configured/i,
    message: "Binance API credentials are not set. Add them in Settings.",
  },
  {
    pattern: /no active trading session/i,
    message: "No trading session is running. Start paper or live trading first.",
  },
  {
    pattern: /data file not found|no such file/i,
    message: "Data file not found. Fetch or select a CSV in Data Studio.",
  },
  {
    pattern: /strategy.*not found/i,
    message: "Strategy file not found. Check the path or browse for a file.",
  },
  {
    pattern: /invalid.*toml|toml parse/i,
    message: "Config file is invalid TOML. Check syntax and try again.",
  },
  {
    pattern: /network|connection|timed out/i,
    message: "Network error. Check your connection and try again.",
  },
];

export function mapUserError(error: unknown): string {
  const raw =
    error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : String(error);

  for (const { pattern, message } of ERROR_PATTERNS) {
    if (pattern.test(raw)) {
      return message;
    }
  }

  return raw.replace(/^Error:\s*/i, "").trim() || "Something went wrong.";
}
