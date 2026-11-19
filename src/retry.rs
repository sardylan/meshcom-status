use std::time::Duration;

pub(crate) const MIN: Duration = Duration::from_secs(2);
pub(crate) const MAX: Duration = Duration::from_secs(60);
pub(crate) const STABLE: Duration = Duration::from_secs(60);

pub(crate) fn next(delay: Duration, lasted: Duration) -> Duration {
    if lasted >= STABLE { MIN } else { (delay * 2).min(MAX) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stable_connection_resets_the_delay() {
        assert_eq!(next(MAX, STABLE), MIN);
        assert_eq!(next(Duration::from_secs(16), Duration::from_secs(3600)), MIN);
    }

    #[test]
    fn a_short_connection_doubles_the_delay() {
        assert_eq!(next(MIN, Duration::from_secs(2)), Duration::from_secs(4));
        assert_eq!(next(Duration::from_secs(8), Duration::ZERO), Duration::from_secs(16));
    }

    #[test]
    fn the_delay_never_exceeds_the_maximum() {
        let mut delay = MIN;
        for _ in 0..10 {
            delay = next(delay, Duration::from_secs(2));
        }
        assert_eq!(delay, MAX);
    }
}
