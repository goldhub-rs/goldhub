//! Sub-tick microsecond timestamped command representations.

use bytemuck::{Pod, Zeroable};

/// Button bitmask flags (aligned with GoldSrc and modern FPS engines).
pub const IN_ATTACK: u32 = 1 << 0;
pub const IN_JUMP: u32 = 1 << 1;
pub const IN_DUCK: u32 = 1 << 2;
pub const IN_FORWARD: u32 = 1 << 3;
pub const IN_BACK: u32 = 1 << 4;
pub const IN_USE: u32 = 1 << 5;
pub const IN_CANCEL: u32 = 1 << 6;
pub const IN_LEFT: u32 = 1 << 7;
pub const IN_RIGHT: u32 = 1 << 8;
pub const IN_MOVELEFT: u32 = 1 << 9;
pub const IN_MOVERIGHT: u32 = 1 << 10;
pub const IN_ATTACK2: u32 = 1 << 11;
pub const IN_RELOAD: u32 = 1 << 12;
pub const IN_SPEED: u32 = 1 << 13;

/// Exactly 64-byte, 1-cache-line user command with microsecond-exact sub-tick timestamping.
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct UserCmd {
    /// Exact wall-clock microsecond timestamp when the input was sampled on the client.
    pub timestamp_micros: u64,
    /// Monotonically increasing command sequence number.
    pub command_number: u32,
    /// Client simulation tick at the time of input.
    pub tick_count: u32,
    /// View angles (pitch, yaw, roll) in degrees.
    pub viewangles: [f32; 3],
    /// Forward movement velocity intent (-400.0 to +400.0).
    pub forwardmove: f32,
    /// Sideways movement velocity intent (-400.0 to +400.0).
    pub sidemove: f32,
    /// Vertical movement velocity intent.
    pub upmove: f32,
    /// Active button bitmask.
    pub buttons: u32,
    /// Impulse command (weapon switch, spray, etc.).
    pub impulse: u8,
    /// Target frame duration in milliseconds.
    pub msec: u8,
    /// Ambient light level.
    pub light_level: u8,
    /// Explicit padding to ensure 64-byte alignment and eliminate false sharing.
    pub _padding: [u8; 17],
}

const _: () = assert!(std::mem::size_of::<UserCmd>() == 64);

impl Default for UserCmd {
    fn default() -> Self {
        Self {
            timestamp_micros: 0,
            command_number: 0,
            tick_count: 0,
            viewangles: [0.0; 3],
            forwardmove: 0.0,
            sidemove: 0.0,
            upmove: 0.0,
            buttons: 0,
            impulse: 0,
            msec: 15, // ~66 Hz default
            light_level: 0,
            _padding: [0; 17],
        }
    }
}

pub const UNACKED_RING_SIZE: usize = 128;
const UNACKED_RING_MASK: usize = UNACKED_RING_SIZE - 1;

/// Fixed-capacity ring buffer of unacknowledged user commands for zero-allocation rollback replay.
#[derive(Debug, Clone)]
pub struct UnackedCmdRing {
    commands: [UserCmd; UNACKED_RING_SIZE],
    newest_cmd_num: u32,
    oldest_unacked_cmd_num: u32,
}

impl Default for UnackedCmdRing {
    fn default() -> Self {
        Self::new()
    }
}

impl UnackedCmdRing {
    /// Creates a new empty command ring.
    pub const fn new() -> Self {
        Self {
            commands: [UserCmd {
                timestamp_micros: 0,
                command_number: 0,
                tick_count: 0,
                viewangles: [0.0; 3],
                forwardmove: 0.0,
                sidemove: 0.0,
                upmove: 0.0,
                buttons: 0,
                impulse: 0,
                msec: 0,
                light_level: 0,
                _padding: [0; 17],
            }; UNACKED_RING_SIZE],
            newest_cmd_num: 0,
            oldest_unacked_cmd_num: 0,
        }
    }

    /// Records a newly sampled user command into the ring buffer.
    pub fn push_cmd(&mut self, cmd: UserCmd) {
        let cmd_num = cmd.command_number;
        let idx = (cmd_num as usize) & UNACKED_RING_MASK;
        self.commands[idx] = cmd;
        self.newest_cmd_num = cmd_num;

        if self.oldest_unacked_cmd_num == 0 {
            self.oldest_unacked_cmd_num = cmd_num;
        }
    }

    /// Acknowledges all commands up to `acked_cmd_num`, retiring them from the unacked ring.
    pub fn acknowledge(&mut self, acked_cmd_num: u32) {
        if acked_cmd_num >= self.oldest_unacked_cmd_num {
            self.oldest_unacked_cmd_num = acked_cmd_num.wrapping_add(1);
        }
    }

    /// Iterates over all unacknowledged commands from `from_cmd_num` to `newest_cmd_num` for replay.
    pub fn get_unacked_range(&self, from_cmd_num: u32) -> Vec<UserCmd> {
        if from_cmd_num > self.newest_cmd_num {
            return Vec::new();
        }

        let count = (self.newest_cmd_num - from_cmd_num + 1) as usize;
        let mut out = Vec::with_capacity(count.min(UNACKED_RING_SIZE));

        for num in from_cmd_num..=self.newest_cmd_num {
            let idx = (num as usize) & UNACKED_RING_MASK;
            let cmd = self.commands[idx];
            if cmd.command_number == num {
                out.push(cmd);
            }
        }

        out
    }

    /// Returns the newest recorded command number.
    #[inline(always)]
    pub const fn newest_cmd_num(&self) -> u32 {
        self.newest_cmd_num
    }

    /// Returns the oldest unacknowledged command number.
    #[inline(always)]
    pub const fn oldest_unacked_cmd_num(&self) -> u32 {
        self.oldest_unacked_cmd_num
    }
}
