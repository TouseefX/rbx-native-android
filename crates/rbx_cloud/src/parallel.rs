//! Bounded parallel fan-out for the blocking client.

/// How many requests [`fan_out`] keeps in flight: enough to turn a
/// hundred sequential round trips into a few seconds, few enough to stay
/// clear of Roblox's per-IP limits (a 429 is retried by `retry` anyway).
const PARALLEL: usize = 8;

/// `f` over every item on up to [`PARALLEL`] threads, results in the
/// items' order. The client is blocking, so this is plain scoped threads.
pub(crate) fn fan_out<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..PARALLEL.min(items.len()) {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else {
                    break;
                };
                let result = f(item);
                results.lock().unwrap_or_else(|e| e.into_inner())[index] = Some(result);
            });
        }
    });
    results
        .into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .into_iter()
        .map(|result| result.expect("every index was taken exactly once"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_out_keeps_the_items_order() {
        let items: Vec<u32> = (0..50).collect();
        let doubled = fan_out(&items, |&n| {
            std::thread::sleep(std::time::Duration::from_micros(u64::from(50 - n)));
            n * 2
        });
        assert_eq!(doubled, (0..50).map(|n| n * 2).collect::<Vec<_>>());
        assert!(fan_out(&[] as &[u32], |&n| n).is_empty());
    }
}
