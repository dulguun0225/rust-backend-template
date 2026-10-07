//! The service binary. Startup errors are `anyhow` here and nowhere else (`layering.toml`, `clippy.toml`).

mod config;

use std::sync::Arc;

use anyhow::Context as _;
use platform::log::{Log, LogEvent, LogField};

static LOG: Log = Log::new(module_path!());

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing::subscriber::set_global_default(platform::log::json_subscriber(std::io::stdout, tracing::Level::INFO))
        .context("installing the log subscriber")?;
    std::panic::set_hook(Box::new(|info| {
        LOG.event_with_cause(LogEvent::ProcessPanic, &info.to_string(), &[]);
    }));

    let config = config::Config::from_lookup(|key| std::env::var(key).ok()).context("reading the configuration")?;
    let tx = db::open(&config.database_url, config.max_connections).await.context("connecting and migrating")?;
    let state = api::AppState { tx, clock: Arc::new(platform::clock::SystemClock) };
    let listener = tokio::net::TcpListener::bind(config.bind).await.context("binding the listener")?;
    LOG.info("listening", &[LogField::count("port", u64::from(config.bind.port()))]);
    web::edge::serve(listener, api::app(state, config.request_body_max_bytes), shutdown()).await.context("serving")?;
    LOG.info("stopped", &[]);
    Ok(())
}

/// Completes on SIGTERM or Ctrl-C.
async fn shutdown() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    first_of(ctrl_c, terminate).await;
}

/// Completes when either future does. Written with `poll_fn` rather than `tokio::select!`, whose expansion
/// uses `%` and so trips `integer_division_remainder_used`, forbidden in this workspace.
async fn first_of(a: impl Future<Output = ()>, b: impl Future<Output = ()>) {
    let mut a = std::pin::pin!(a);
    let mut b = std::pin::pin!(b);
    std::future::poll_fn(|cx| {
        if a.as_mut().poll(cx).is_ready() || b.as_mut().poll(cx).is_ready() {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use std::future::{pending, ready};
    use std::task::{Context, Poll, Waker};

    fn polled_once(f: impl Future<Output = ()>) -> Poll<()> {
        let mut f = std::pin::pin!(f);
        f.as_mut().poll(&mut Context::from_waker(Waker::noop()))
    }

    #[tokio::test]
    async fn shutdown_waits_for_a_signal() {
        assert!(polled_once(super::shutdown()).is_pending());
    }

    #[test]
    fn either_future_completing_completes_the_pair() {
        assert!(polled_once(super::first_of(ready(()), pending())).is_ready());
        assert!(polled_once(super::first_of(pending(), ready(()))).is_ready());
        assert!(polled_once(super::first_of(pending::<()>(), pending())).is_pending());
    }
}
