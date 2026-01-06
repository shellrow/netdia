use std::{
    collections::HashMap,
    sync::Arc,
    time::{Instant, SystemTime},
};

use crate::{
    net,
    state::{AppState, IfStats},
};
use anyhow::Result;

pub async fn reload_interfaces(state: &Arc<AppState>) -> Result<()> {
    let list = net::interface::list_interfaces();
    let mut map = HashMap::new();

    for iface in list {
        map.insert(iface.index, iface);
    }

    {
        let mut ifaces = state.interfaces.lock().await;
        *ifaces = map;
    }

    {
        let mut last = state.last_refresh.lock().await;
        *last = SystemTime::now();
    }

    Ok(())
}

// Update interface stats and calculate bps
pub async fn update_interface_state(state: &Arc<AppState>) -> Result<()> {
    // Acquire locks
    let mut ifaces = state.interfaces.lock().await;
    let mut stats_cache = state.stats.lock().await;

    for (ifindex, iface) in ifaces.iter_mut() {
        // Update oper_state and stats
        iface.update_oper_state();
        iface.update_stats()?;
        let tick_ts = Instant::now();

        // Calculate bps if previous stats exist
        if let Some(s) = &iface.stats {
            let prev = stats_cache.get(ifindex);

            // Calculate bps
            if let Some(prev) = prev {
                let elapsed = tick_ts.duration_since(prev.ts);
                if elapsed.as_secs_f64() > 0.0 {
                    let rx_bytes_per_sec =
                        (s.rx_bytes.saturating_sub(prev.rx_bytes)) as f64 / elapsed.as_secs_f64();
                    let tx_bytes_per_sec =
                        (s.tx_bytes.saturating_sub(prev.tx_bytes)) as f64 / elapsed.as_secs_f64();

                    // Save to cache
                    stats_cache.insert(
                        *ifindex,
                        IfStats {
                            rx_bytes: s.rx_bytes,
                            tx_bytes: s.tx_bytes,
                            rx_bytes_per_sec,
                            tx_bytes_per_sec,
                            ts: tick_ts,
                        },
                    );
                }
            } else {
                // No previous stats, initialize with 0 bps
                stats_cache.insert(
                    *ifindex,
                    IfStats {
                        rx_bytes: s.rx_bytes,
                        tx_bytes: s.tx_bytes,
                        rx_bytes_per_sec: 0.0,
                        tx_bytes_per_sec: 0.0,
                        ts: tick_ts,
                    },
                );
            }
        }
    }

    Ok(())
}
