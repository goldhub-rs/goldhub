//! Server-to-Client (`svc_*`) and Client-to-Server (`clc_*`) protocol opcodes and parsers.

use crate::error::NetError;

/// Server to Client command opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SvcOpcode {
    Bad = 0,
    Nop = 1,
    Disconnect = 2,
    Event = 3,
    Version = 4,
    SetView = 5,
    Sound = 6,
    Time = 7,
    Print = 8,
    StuffText = 9,
    SetAngle = 10,
    ServerInfo = 11,
    LightStyle = 12,
    UpdateUserInfo = 13,
    DeltaDescription = 14,
    ClientData = 15,
    StopSound = 16,
    Pings = 17,
    Particle = 18,
    Damage = 19,
    SpawnStatic = 20,
    EventReliable = 21,
    SpawnBaseline = 22,
    TempEntity = 23,
    SetPause = 24,
    SignonNum = 25,
    CenterPrint = 26,
    KilledMonster = 27,
    FoundSecret = 28,
    SpawnStaticSound = 29,
    Intermission = 30,
    Finale = 31,
    CdTrack = 32,
    Restore = 33,
    Cutscene = 34,
    WeaponAnim = 35,
    DecalName = 36,
    RoomType = 37,
    AddAngle = 38,
    NewUserMsg = 39,
    PacketEntities = 40,
    DeltaPacketEntities = 41,
    Choke = 42,
    ResourceList = 43,
    NewMoveVars = 44,
    ResourceRequest = 45,
    Customization = 46,
    CrosshairAngle = 47,
    SoundFade = 48,
    FileTxferFailed = 49,
    Hltv = 50,
    Director = 51,
    VoiceInit = 52,
    VoiceData = 53,
    SendExtraInfo = 54,
    TimeScale = 55,
    ResourceLocation = 56,
    SendCvarValue = 57,
    SendCvarValue2 = 58,
    Exec = 59,
    Unknown(u8),
}

impl From<u8> for SvcOpcode {
    fn from(b: u8) -> Self {
        match b {
            0 => Self::Bad,
            1 => Self::Nop,
            2 => Self::Disconnect,
            3 => Self::Event,
            4 => Self::Version,
            5 => Self::SetView,
            6 => Self::Sound,
            7 => Self::Time,
            8 => Self::Print,
            9 => Self::StuffText,
            10 => Self::SetAngle,
            11 => Self::ServerInfo,
            12 => Self::LightStyle,
            13 => Self::UpdateUserInfo,
            14 => Self::DeltaDescription,
            15 => Self::ClientData,
            16 => Self::StopSound,
            17 => Self::Pings,
            18 => Self::Particle,
            19 => Self::Damage,
            20 => Self::SpawnStatic,
            21 => Self::EventReliable,
            22 => Self::SpawnBaseline,
            23 => Self::TempEntity,
            24 => Self::SetPause,
            25 => Self::SignonNum,
            26 => Self::CenterPrint,
            27 => Self::KilledMonster,
            28 => Self::FoundSecret,
            29 => Self::SpawnStaticSound,
            30 => Self::Intermission,
            31 => Self::Finale,
            32 => Self::CdTrack,
            33 => Self::Restore,
            34 => Self::Cutscene,
            35 => Self::WeaponAnim,
            36 => Self::DecalName,
            37 => Self::RoomType,
            38 => Self::AddAngle,
            39 => Self::NewUserMsg,
            40 => Self::PacketEntities,
            41 => Self::DeltaPacketEntities,
            42 => Self::Choke,
            43 => Self::ResourceList,
            44 => Self::NewMoveVars,
            45 => Self::ResourceRequest,
            46 => Self::Customization,
            47 => Self::CrosshairAngle,
            48 => Self::SoundFade,
            49 => Self::FileTxferFailed,
            50 => Self::Hltv,
            51 => Self::Director,
            52 => Self::VoiceInit,
            53 => Self::VoiceData,
            54 => Self::SendExtraInfo,
            55 => Self::TimeScale,
            56 => Self::ResourceLocation,
            57 => Self::SendCvarValue,
            58 => Self::SendCvarValue2,
            59 => Self::Exec,
            other => Self::Unknown(other),
        }
    }
}

/// Client to Server command opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ClcOpcode {
    Bad = 0,
    Nop = 1,
    Move = 2,
    StringCmd = 3,
    Delta = 4,
    ResourceList = 5,
    TMove = 6,
    FileConsistency = 7,
    VoiceData = 8,
    Hltv = 9,
    CvarValue = 10,
    CvarValue2 = 11,
    Unknown(u8),
}

impl From<u8> for ClcOpcode {
    fn from(b: u8) -> Self {
        match b {
            0 => Self::Bad,
            1 => Self::Nop,
            2 => Self::Move,
            3 => Self::StringCmd,
            4 => Self::Delta,
            5 => Self::ResourceList,
            6 => Self::TMove,
            7 => Self::FileConsistency,
            8 => Self::VoiceData,
            9 => Self::Hltv,
            10 => Self::CvarValue,
            11 => Self::CvarValue2,
            other => Self::Unknown(other),
        }
    }
}

/// Helper function to read a null-terminated C string from a byte slice.
pub fn read_cstring<'a>(bytes: &'a [u8], offset: &mut usize) -> Result<&'a str, NetError> {
    let start = *offset;
    if start >= bytes.len() {
        return Err(NetError::MalformedPayload {
            opcode: 0,
            reason: "unexpected end of buffer while reading string".into(),
        });
    }

    let end =
        bytes[start..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| NetError::MalformedPayload {
                opcode: 0,
                reason: "unterminated C string".into(),
            })?;

    *offset = start + end + 1; // skip null terminator
    std::str::from_utf8(&bytes[start..start + end]).map_err(|e| NetError::MalformedPayload {
        opcode: 0,
        reason: format!("invalid UTF-8 string: {e}"),
    })
}
