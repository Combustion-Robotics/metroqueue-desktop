//! Receiver side of pairing, a copy of MetroQueue's `TvPairing` (PROTOCOL.md §4): a short-lived
//! 6-digit code shown on this screen, rate limits and a lockout, and a long-lived token of which
//! only the SHA-256 is kept. Pure: time comes in as `now`, so the limits are testable.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use base64::Engine;
use rand::{Rng, RngCore};
use sha2::{Digest, Sha256};

pub const CODE_TTL: Duration = Duration::from_secs(2 * 60);
const START_WINDOW: Duration = Duration::from_secs(60);
const MAX_STARTS_PER_WINDOW: usize = 3;
const MAX_ATTEMPTS_PER_CODE: u32 = 5;
const LOCKOUT_WINDOW: Duration = Duration::from_secs(10 * 60);
const MAX_FAILURES_PER_WINDOW: usize = 10;
const LOCKOUT: Duration = Duration::from_secs(5 * 60);
const MAX_NAME_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub code: String,
    pub device_name: String,
    expires: Instant,
}

#[derive(Debug, PartialEq)]
pub enum Finish {
    Paired,
    WrongCode,
    /// No code, an expired one, or the last of its tries used up: `410`.
    Expired,
    Locked,
}

#[derive(Default)]
pub struct Pairing {
    prompt: Option<Prompt>,
    starts: VecDeque<Instant>,
    failures: VecDeque<Instant>,
    attempts_left: u32,
    locked_until: Option<Instant>,
}

impl Pairing {
    /// A new code replacing any previous one. `None` when asked too often or locked out.
    pub fn start(&mut self, device_name: &str, now: Instant) -> Option<&Prompt> {
        drop_older(&mut self.starts, now, START_WINDOW);
        if self.locked(now) || self.starts.len() >= MAX_STARTS_PER_WINDOW {
            return None;
        }
        self.starts.push_back(now);
        self.attempts_left = MAX_ATTEMPTS_PER_CODE;
        let code = format!("{:06}", rand::rngs::OsRng.gen_range(0..1_000_000));
        self.prompt =
            Some(Prompt { code, device_name: clean_name(device_name), expires: now + CODE_TTL });
        self.prompt.as_ref()
    }

    pub fn finish(&mut self, code: &str, now: Instant) -> Finish {
        if self.locked(now) {
            return Finish::Locked;
        }
        let Some(prompt) = self.prompt.as_ref().filter(|p| now <= p.expires) else {
            self.prompt = None;
            return Finish::Expired;
        };
        if ct_eq(code.trim().as_bytes(), prompt.code.as_bytes()) {
            self.prompt = None;
            return Finish::Paired;
        }
        drop_older(&mut self.failures, now, LOCKOUT_WINDOW);
        self.failures.push_back(now);
        self.attempts_left = self.attempts_left.saturating_sub(1);
        if self.failures.len() >= MAX_FAILURES_PER_WINDOW {
            self.locked_until = Some(now + LOCKOUT);
            self.failures.clear();
            self.prompt = None;
            return Finish::Locked;
        }
        if self.attempts_left == 0 {
            self.prompt = None;
            return Finish::Expired;
        }
        Finish::WrongCode
    }

    /// Drop the code (the dialog was dismissed). The name it was for, if there was one.
    pub fn cancel(&mut self) -> Option<String> {
        self.prompt.take().map(|p| p.device_name)
    }

    /// Drop the code if it is still `code`, for the expiry timer. Whether it was.
    pub fn expire(&mut self, code: &str) -> bool {
        if self.prompt.as_ref().is_some_and(|p| p.code == code) {
            self.prompt = None;
            return true;
        }
        false
    }

    fn locked(&self, now: Instant) -> bool {
        self.locked_until.is_some_and(|t| now < t)
    }
}

fn drop_older(times: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while times.front().is_some_and(|t| now.duration_since(*t) > window) {
        times.pop_front();
    }
}

/// The phone's name as shown here: trimmed, at most 40 characters, `"Phone"` when empty.
pub fn clean_name(name: &str) -> String {
    let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() {
        "Phone".into()
    } else {
        name
    }
}

/// 32 random bytes, Base64 URL-safe without padding (43 chars).
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// SHA-256 of the token's UTF-8 bytes, lowercase hex: all that is stored.
pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant time for equal lengths (the lengths themselves are not secret).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A random (v4) UUID, for this receiver's id and each paired phone's.
pub fn new_uuid() -> String {
    let mut b = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(p: &mut Pairing, now: Instant) -> String {
        p.start("Pixel 9", now).unwrap().code.clone()
    }

    fn wrong(code: &str) -> String {
        format!("{:06}", (code.parse::<u32>().unwrap() + 1) % 1_000_000)
    }

    #[test]
    fn right_code_pairs_once() {
        let now = Instant::now();
        let mut p = Pairing::default();
        let c = code(&mut p, now);
        assert_eq!(c.len(), 6);
        assert!(c.chars().all(|ch| ch.is_ascii_digit()));
        assert_eq!(p.finish(&format!(" {c}\n"), now), Finish::Paired);
        assert_eq!(p.finish(&c, now), Finish::Expired);
    }

    #[test]
    fn code_expires_after_two_minutes() {
        let now = Instant::now();
        let mut p = Pairing::default();
        let c = code(&mut p, now);
        assert_eq!(p.finish(&c, now + CODE_TTL + Duration::from_secs(1)), Finish::Expired);
    }

    #[test]
    fn three_starts_per_minute() {
        let now = Instant::now();
        let mut p = Pairing::default();
        for _ in 0..3 {
            assert!(p.start("x", now).is_some());
        }
        assert!(p.start("x", now).is_none());
        assert!(p.start("x", now + Duration::from_secs(61)).is_some());
    }

    #[test]
    fn fifth_wrong_try_discards_the_code() {
        let now = Instant::now();
        let mut p = Pairing::default();
        let c = code(&mut p, now);
        for _ in 0..4 {
            assert_eq!(p.finish(&wrong(&c), now), Finish::WrongCode);
        }
        assert_eq!(p.finish(&wrong(&c), now), Finish::Expired);
        assert_eq!(p.finish(&c, now), Finish::Expired);
    }

    #[test]
    fn ten_wrong_codes_lock_for_five_minutes() {
        let now = Instant::now();
        let mut p = Pairing::default();
        let c = code(&mut p, now);
        for _ in 0..5 {
            p.finish(&wrong(&c), now);
        }
        let c = code(&mut p, now);
        for _ in 0..4 {
            assert_eq!(p.finish(&wrong(&c), now), Finish::WrongCode);
        }
        assert_eq!(p.finish(&wrong(&c), now), Finish::Locked);
        let later = now + Duration::from_secs(61);
        assert!(p.start("x", later).is_none());
        assert_eq!(p.finish("000000", later), Finish::Locked);
        assert!(p.start("x", now + LOCKOUT + Duration::from_secs(1)).is_some());
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  "), "Phone");
        assert_eq!(clean_name(" Pixel 9 "), "Pixel 9");
        assert_eq!(clean_name(&"é".repeat(50)).chars().count(), 40);
    }

    #[test]
    fn tokens_and_hashes() {
        let t = new_token();
        assert_eq!(t.len(), 43);
        assert!(t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        assert_ne!(t, new_token());
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
        let id = new_uuid();
        assert_eq!((id.len(), &id[14..15]), (36, "4"));
    }
}
