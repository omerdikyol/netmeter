//! Desktop notifications for data-cap thresholds.

/// Post a notification.
///
/// Failures are logged rather than fatal: a notification that cannot be shown
/// must never take the sampler down.
pub fn send(summary: &str, body: &str) {
    if let Err(err) = notify_rust::Notification::new()
        .summary(summary)
        .body(body)
        .show()
    {
        eprintln!("netmeter: could not post a notification: {err}");
    }
}
