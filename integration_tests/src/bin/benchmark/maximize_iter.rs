use std::iter;

use tracing::warn;

use crate::args::{Resolution, ResolutionPreset};

pub trait MaximizeIter<T> {
    fn next(&mut self, prev_success: bool) -> Option<T>;
}

pub struct Const<T: Clone + Copy>(pub T);

impl<T: Clone + Copy> MaximizeIter<T> for Const<T> {
    fn next(&mut self, _prev_success: bool) -> Option<T> {
        Some(self.0)
    }
}

pub struct MaximizeU64 {
    called_once: bool,
    last_value: u64,
    upper_bound: Option<u64>,
    lower_bound: u64,
    precision: u64,
}

impl MaximizeU64 {
    pub fn new(init_value: u64) -> Self {
        Self::new_with_precision(init_value, 1)
    }

    pub fn new_with_precision(init_value: u64, precision: u64) -> Self {
        Self {
            called_once: false,
            last_value: init_value,
            upper_bound: None,
            lower_bound: 0,
            precision,
        }
    }
}

impl MaximizeIter<u64> for MaximizeU64 {
    fn next(&mut self, prev_success: bool) -> Option<u64> {
        if prev_success {
            self.lower_bound = u64::max(self.lower_bound, self.last_value)
        }
        if !self.called_once {
            self.called_once = true;
            return Some(self.last_value);
        }
        match self.upper_bound {
            None => match prev_success {
                true => {
                    self.last_value = match self.last_value {
                        0 => 1,
                        value => value * 2,
                    };
                }
                false => {
                    self.upper_bound = Some(self.last_value);
                    self.last_value = (self.last_value + self.lower_bound) / 2;
                }
            },
            Some(upper_bound) => {
                if upper_bound - self.lower_bound <= self.precision {
                    return None;
                };

                match prev_success {
                    true => {
                        self.lower_bound = self.last_value;
                        self.last_value = (self.last_value + upper_bound) / 2;
                    }
                    false => {
                        self.upper_bound = Some(self.last_value);
                        self.last_value = (self.last_value + self.lower_bound) / 2;
                    }
                }
            }
        };
        Some(self.last_value)
    }
}

pub struct MaximizeResolution {
    last_index: usize,
    resolutions: Vec<Resolution>,
}

impl MaximizeResolution {
    pub fn new() -> Self {
        let preset_iter = ResolutionPreset::iter().map(Resolution::from);
        let first = iter::once(Resolution {
            width: 1,
            height: 1,
        });
        Self {
            last_index: 0,
            resolutions: first.chain(preset_iter).collect(),
        }
    }
}

impl MaximizeIter<Resolution> for MaximizeResolution {
    fn next(&mut self, prev_success: bool) -> Option<Resolution> {
        match prev_success {
            true => {
                if self.resolutions.len() <= self.last_index + 1 {
                    warn!("Reached max value when searching max value");
                    return None;
                };
                self.last_index += 1;
                Some(self.resolutions[self.last_index])
            }
            false => None,
        }
    }
}
