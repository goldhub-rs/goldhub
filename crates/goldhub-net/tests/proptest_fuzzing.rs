use bytemuck::{bytes_of, from_bytes};
use goldhub_net::prediction::UserCmd;
use goldhub_net::protocol::netchan::NetChannelHeader;
use goldhub_net::protocol::oob::OobMessage;
use goldhub_net::ray_intersects_aabb;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn fuzz_netchannel_header_decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let _ = NetChannelHeader::decode(&bytes);
    }

    #[test]
    fn fuzz_oob_message_decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let _ = OobMessage::decode(&bytes);
    }

    #[test]
    fn fuzz_user_cmd_pod_roundtrip(
        timestamp_micros in any::<u64>(),
        command_number in any::<u32>(),
        forwardmove in any::<f32>(),
        sidemove in any::<f32>(),
        buttons in any::<u32>(),
    ) {
        let cmd = UserCmd {
            timestamp_micros,
            command_number,
            forwardmove,
            sidemove,
            buttons,
            ..Default::default()
        };

        let bytes = bytes_of(&cmd);
        assert_eq!(bytes.len(), 64);

        let decoded: &UserCmd = from_bytes(bytes);
        assert_eq!(decoded.timestamp_micros, timestamp_micros);
        assert_eq!(decoded.command_number, command_number);
        assert_eq!(decoded.buttons, buttons);
    }

    #[test]
    fn fuzz_ray_aabb_intersection_never_panics(
        origin_x in -1000.0f32..1000.0,
        origin_y in -1000.0f32..1000.0,
        origin_z in -1000.0f32..1000.0,
        dir_x in -1.0f32..1.0,
        dir_y in -1.0f32..1.0,
        dir_z in -1.0f32..1.0,
        min_x in -500.0f32..0.0,
        min_y in -500.0f32..0.0,
        min_z in -500.0f32..0.0,
        max_x in 0.0f32..500.0,
        max_y in 0.0f32..500.0,
        max_z in 0.0f32..500.0,
    ) {
        let origin = [origin_x, origin_y, origin_z];
        let dir = [dir_x, dir_y, dir_z];
        let min = [min_x, min_y, min_z];
        let max = [max_x, max_y, max_z];

        if let Some(dist) = ray_intersects_aabb(origin, dir, min, max) {
            assert!(dist.is_finite());
            assert!(dist >= 0.0);
        }
    }
}
