interface Props {
  message: string;
  variant?: "info" | "error" | "success";
  onDismiss?: () => void;
}

export function StatusBanner({
  message,
  variant = "info",
  onDismiss,
}: Props) {
  if (!message) return null;

  return (
    <div
      className={`status-banner status-banner-${variant}`}
      role={variant === "error" ? "alert" : "status"}
      aria-live={variant === "error" ? "assertive" : "polite"}
    >
      <p className={variant === "error" ? "status-error" : "status"}>{message}</p>
      {onDismiss && (
        <button type="button" className="secondary" onClick={onDismiss}>
          Dismiss
        </button>
      )}
    </div>
  );
}
