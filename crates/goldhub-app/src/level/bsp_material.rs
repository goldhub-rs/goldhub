//! Custom GoldSrc BSP World Material eliminating PBR pipeline in favor of authentic `Texture * Lightmap * 2.0` overbrighting.
#![allow(dead_code, unused)]

use bevy::asset::load_internal_asset;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::mesh::MeshVertexBufferLayoutRef;
use bevy::render::render_resource::{
    AsBindGroup, Face, RenderPipelineDescriptor, Shader, ShaderRef, ShaderType,
    SpecializedMeshPipelineError,
};

pub const GOLDSRC_BSP_SHADER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(0x0090_1d52_cb59_0001_dead_beef_0001);

#[derive(ShaderType, Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct GoldSrcBspMaterialUniform {
    pub overbright: f32,
    pub alpha_cutoff: f32,
    pub render_mode: u32,
    pub render_amt: f32,
    pub flashlight_active: u32,
    pub flashlight_radius: f32,
    pub flashlight_intensity: f32,
    pub flashlight_pos: Vec3,
    pub flashlight_dir: Vec3,
    pub water_alpha: f32,
    pub time: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GoldSrcBspMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub diffuse_texture: Handle<Image>,

    #[texture(2)]
    #[sampler(3)]
    pub lightmap_texture: Handle<Image>,

    #[uniform(4)]
    pub uniform: GoldSrcBspMaterialUniform,

    pub cull_mode: Option<Face>,
    pub alpha_mode: AlphaMode,
}

impl Material for GoldSrcBspMaterial {
    fn fragment_shader() -> ShaderRef {
        GOLDSRC_BSP_SHADER_HANDLE.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Transparent surfaces (glass windows) render without backface culling in GoldSrc
        // so they are visible from both inside and outside. Opaque and mask world geometry
        // use standard backface culling.
        descriptor.primitive.cull_mode = match descriptor.primitive.cull_mode {
            None => None,
            _ => Some(Face::Back),
        };
        Ok(())
    }
}

pub struct GoldSrcBspMaterialPlugin;

impl Plugin for GoldSrcBspMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            GOLDSRC_BSP_SHADER_HANDLE,
            "goldsrc_bsp.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(bevy::pbr::MaterialPlugin::<GoldSrcBspMaterial> {
            shadows_enabled: false,
            prepass_enabled: false,
            ..default()
        });
    }
}
