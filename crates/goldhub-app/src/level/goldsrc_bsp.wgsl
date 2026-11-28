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
    flashlight_intensity: f32,
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

    // GoldSrc Dynamic Flashlight (CL_UpdateFlashlight + R_AddDynamicLights parity):
    // Dynamic light accumulates onto incoming lightmap illumination BEFORE diffuse multiplication
    // so textures retain full contrast and never bleach to white.
    if (material.flashlight_active == 1u) {
        let to_light = material.flashlight_pos - in.world_position.xyz;
        let dist = length(to_light);
        if (dist < material.flashlight_radius) {
            let l_dir = normalize(to_light);
            let n = select(-in.world_normal, in.world_normal, is_front);
            let NdotL = dot(n, l_dir);
            if (NdotL > 0.0) {
                let radial = 1.0 - (dist / material.flashlight_radius);
                let dlight = radial * radial * (0.45 + 0.55 * NdotL) * material.flashlight_intensity;
                lm += vec3<f32>(dlight);
            }
        }
    }

    // Authentic GoldSrc radiosity curve:
    // Lightmap is sampled as unorm luxels directly modulating diffuse with overbright.
    let rgb = clamp(diffuse.rgb * lm * material.overbright, vec3<f32>(0.0), vec3<f32>(1.0));

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
