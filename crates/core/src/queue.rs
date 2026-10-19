//! Talking to a headset one request at a time.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

const RETRY_AFTER: Duration = Duration::from_millis(1000);
const MAX_RETRIES: u8 = 2;

/// Requests waiting to go to the headset. Only one is in flight: the next
/// goes out once the headset answers it. One that gets no answer is resent a
/// couple of times, then dropped so the rest keep moving.
#[derive(Debug)]
pub(crate) struct StopAndWait<R> {
    queue: VecDeque<R>,
    in_flight: Option<InFlight<R>>,
}

#[derive(Debug)]
struct InFlight<R> {
    request: R,
    sent_at: Instant,
    retries: u8,
}

impl<R> Default for StopAndWait<R> {
    fn default() -> Self {
        Self { queue: VecDeque::new(), in_flight: None }
    }
}

impl<R: Clone> StopAndWait<R> {
    /// Queues a request after dropping the queued ones it supersedes, so a
    /// dragged slider doesn't build a backlog.
    pub fn push(&mut self, request: R, supersedes: impl Fn(&R) -> bool) {
        self.queue.retain(|queued| !supersedes(queued));
        self.queue.push_back(request);
    }

    pub fn in_flight(&self) -> Option<&R> {
        self.in_flight.as_ref().map(|in_flight| &in_flight.request)
    }

    /// The headset answered the request in flight.
    pub fn complete(&mut self) {
        self.in_flight = None;
    }

    /// The next request to send, unless one is still in flight.
    pub fn next(&mut self, now: Instant) -> Option<R> {
        if self.in_flight.is_some() {
            return None;
        }
        let request = self.queue.pop_front()?;
        self.in_flight = Some(InFlight { request: request.clone(), sent_at: now, retries: 0 });
        Some(request)
    }

    /// The request in flight again if it went unanswered for too long, or the
    /// next one once it's been given up on.
    pub fn tick(&mut self, now: Instant) -> Option<R> {
        if let Some(in_flight) = &mut self.in_flight
            && now.duration_since(in_flight.sent_at) >= RETRY_AFTER
        {
            if in_flight.retries < MAX_RETRIES {
                in_flight.retries += 1;
                in_flight.sent_at = now;
                return Some(in_flight.request.clone());
            }
            self.in_flight = None;
        }
        self.next(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_one_at_a_time() {
        let now = Instant::now();
        let mut queue = StopAndWait::default();
        queue.push(1, |_| false);
        queue.push(2, |_| false);
        assert_eq!(queue.next(now), Some(1));
        assert_eq!(queue.next(now), None);
        queue.complete();
        assert_eq!(queue.next(now), Some(2));
    }

    #[test]
    fn drops_superseded_requests() {
        let now = Instant::now();
        let mut queue = StopAndWait::default();
        queue.push(0, |_| false);
        assert_eq!(queue.next(now), Some(0));
        for level in [5, 10, 15] {
            queue.push(level, |_| true);
        }
        queue.complete();
        assert_eq!(queue.next(now), Some(15));
    }

    #[test]
    fn retries_then_moves_on() {
        let start = Instant::now();
        let mut queue = StopAndWait::default();
        queue.push("a", |_| false);
        queue.push("b", |_| false);
        assert_eq!(queue.next(start), Some("a"));
        assert_eq!(queue.tick(start + Duration::from_millis(500)), None);
        assert_eq!(queue.tick(start + RETRY_AFTER), Some("a"));
        assert_eq!(queue.tick(start + RETRY_AFTER * 2), Some("a"));
        assert_eq!(queue.tick(start + RETRY_AFTER * 3), Some("b"));
    }
}
