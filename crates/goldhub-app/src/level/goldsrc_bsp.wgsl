#import bevy_pbr::forward_io::{VertexOutput, FragmentOutput}

@group(2) @binding(0) var diffuse_texture: texture_2d<f32>;
@group(2) @binding(1) var diffuse_sampler: sampler;
@group(2) @binding(2) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(3) var lightmap_sampler: sampler;

struct GoldSrcBspMaterialUniform {
    overbright: f32,
    alpha_cutoff: f32,
    render_mode: u32,
    render_amt: f32,
    flashlight_active: u32,
    flashlight_radius: f32,
    flashlight_pos: vec3<f32>,
    flashlight_dir: vec3<f32>,
    water_alpha: f32,
    time: f32,
};
@group(2) @binding(4) var<uniform> material: GoldSrcBspMaterialUniform;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var out: FragmentOutput;
    var uv = in.uv;

    // Mode 5: Water surface with animated sine ripple displacement
    if (material.render_mode == 5u) {
        let ripple = vec2<f32>(
            sin(in.world_position.x * 0.04 + material.time * 2.5),
            cos(in.world_position.z * 0.04 + material.time * 2.5)
        ) * 0.015;
        uv = uv + ripple;
    }

    let diffuse = textureSample(diffuse_texture, diffuse_sampler, uv);

    // Mode 4 / Cutout: Alpha test discard
    if (material.render_mode == 4u && diffuse.a < material.alpha_cutoff) {
        discard;
    }

    let lightmap = textureSample(lightmap_texture, lightmap_sampler, in.uv_b);

    // Mode 2 / 3: Translucent unlit (glass in GoldSrc has no lightmap, pure diffuse)
    var lm = lightmap.rgb;
    if (material.render_mode == 2u || material.render_mode == 3u) {
        lm = vec3<f32>(1.0 / material.overbright);
    }

    // Authentic GoldSrc radiosity curve:
    // Lightmap is sampled as unorm luxels directly modulating diffuse with overbright.
    var rgb = diffuse.rgb * lm * material.overbright;

    // GoldSrc Dynamic Flashlight (dlight_t spot projection directly on BSP world surfaces)
    if (material.flashlight_active == 1u) {
        let to_frag = in.world_position.xyz - material.flashlight_pos;
        let dist = length(to_frag);
        if (dist < material.flashlight_radius && dist > 1.0) {
            let l_dir = to_frag / dist;
            let cos_angle = dot(l_dir, material.flashlight_dir);
            // Spot cone: 18° inner to 24° outer falloff
            if (cos_angle > 0.90) {
                let spot = smoothstep(0.90, 0.96, cos_angle);
                let dist_falloff = 1.0 - (dist / material.flashlight_radius);
                let NdotL = max(dot(in.world_normal, -l_dir), 0.0);
                let intensity = spot * dist_falloff * dist_falloff * (NdotL * 0.7 + 0.3) * 1.6;
                rgb += vec3<f32>(intensity);
            }
        }
    }

    var alpha = diffuse.a;
    if (material.render_mode == 2u) {
        // Texture alpha transparency (glass)
        alpha = alpha * (material.render_amt / 255.0);
    } else if (material.render_mode == 5u) {
        // Transparent water (r_wateralpha parity)
        alpha = material.water_alpha;
    }

    out.color = vec4<f32>(rgb, alpha);
    return out;
}
