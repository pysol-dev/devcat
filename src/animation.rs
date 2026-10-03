use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Idle,
    Left,
    Right,
}

pub struct Animation {
    next_left: bool,
    pose: Pose,
    deadline: Option<Instant>,
}
impl Animation {
    pub const fn new() -> Self {
        Self {
            next_left: true,
            pose: Pose::Idle,
            deadline: None,
        }
    }
    pub fn strike(&mut self, now: Instant) {
        self.pose = if self.next_left {
            Pose::Left
        } else {
            Pose::Right
        };
        self.next_left = !self.next_left;
        self.deadline = now.checked_add(Duration::from_millis(110));
    }
    pub fn pose(&mut self, now: Instant) -> Pose {
        if self.deadline.is_some_and(|end| now >= end) {
            self.pose = Pose::Idle;
            self.deadline = None;
        }
        self.pose
    }
}
impl Default for Animation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternating_and_bounded() {
        let t = Instant::now();
        let mut a = Animation::new();
        a.strike(t);
        assert_eq!(a.pose(t), Pose::Left);
        a.strike(t);
        assert_eq!(a.pose(t), Pose::Right);
        assert_eq!(a.pose(t + Duration::from_secs(1)), Pose::Idle);
    }
}
