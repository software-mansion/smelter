use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

pub(crate) fn random_string(length: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect::<String>()
}
