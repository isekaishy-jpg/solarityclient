//! One retained native sky simulation for the active map's world frames.

use crate::application::environment_coordinator::RuntimeWorldEnvironmentFrame;
use crate::application::frame_profile::RuntimeFrameProfile;
use solarity_rendering::{
    BlpTextureHandle, WorldCameraFrame, WorldCelestialDraw, WorldCelestialFrame,
    WorldCelestialMesh, WorldCelestials, WorldCloudDome, WorldCloudFrame, WorldCloudLighting,
    WorldClouds, WorldSkyDome, WorldSkyFrame,
};

pub(super) struct WorldSky {
    gradient: WorldSkyDome,
    cloud_dome: WorldCloudDome,
    clouds: WorldClouds,
    last_update_ms: Option<u32>,
    elapsed_seconds: f32,
    celestials: WorldCelestials,
    celestial_meshes: [WorldCelestialMesh; 3],
}

impl WorldSky {
    pub(super) fn new() -> Self {
        let celestials = WorldCelestials::sample(0., 0., glam::Vec3::ZERO);
        Self {
            celestials,
            celestial_meshes: celestials
                .bodies()
                .map(|body| WorldCelestialMesh::new(body, glam::Vec3::ZERO, 0)),
            gradient: WorldSkyDome::new(),
            cloud_dome: WorldCloudDome::new(),
            clouds: WorldClouds::new(1),
            last_update_ms: None,
            elapsed_seconds: 0.,
        }
    }

    pub(super) fn update(
        &mut self,
        environment: RuntimeWorldEnvironmentFrame,
        camera: WorldCameraFrame,
        time_ms: u32,
        celestial_colors: [u32; 3],
    ) {
        let mut profile = RuntimeFrameProfile::new("World sky update");
        let light = environment.light();
        self.gradient.update_colors(
            light.sky_colors(),
            light.fog_color(),
            environment.day_fraction(),
            light.highlight_sky(),
            camera,
        );
        profile.mark("gradient colors");
        let celestials = WorldCelestials::sample(
            environment.day_fraction(),
            environment.calendar_days() as f32,
            camera.camera().position(),
        );
        let [sun, moon, _] = celestials.bodies();
        self.celestials = celestials;
        self.celestial_meshes = std::array::from_fn(|i| {
            WorldCelestialMesh::new(
                celestials.bodies()[i],
                camera.camera().position(),
                celestial_colors[i],
            )
        });
        profile.mark("celestials");
        let lighting = WorldCloudLighting::sample(
            light.cloud_colors(),
            environment.day_fraction(),
            camera.camera().position(),
            sun.position(),
            moon.position(),
            environment.weather_blend(),
        );
        profile.mark("cloud lighting");
        let elapsed = self
            .last_update_ms
            .map_or(0., |previous| time_ms.wrapping_sub(previous) as f32 * 0.001);
        self.clouds.update(elapsed, light.sky_floats()[1], lighting);
        self.elapsed_seconds = elapsed;
        profile.mark("cloud rows");
        self.last_update_ms = Some(time_ms);
    }

    pub(super) fn gradient_frame(&self, camera: WorldCameraFrame) -> WorldSkyFrame<'_> {
        WorldSkyFrame::new(&self.gradient, camera)
    }
    pub(super) fn celestial_frame(
        &self,
        camera: WorldCameraFrame,
        textures: [BlpTextureHandle; 3],
    ) -> WorldCelestialFrame<'_> {
        WorldCelestialFrame::new(std::array::from_fn(|i| {
            WorldCelestialDraw::new(
                &self.celestial_meshes[i],
                self.celestials.bodies()[i],
                textures[i],
                camera,
            )
        }))
    }
    pub(super) fn cloud_frame(&self, camera: WorldCameraFrame) -> WorldCloudFrame<'_> {
        WorldCloudFrame::new(&self.cloud_dome, &self.clouds, camera)
    }

    /// Supplies glare after the world, with separate sky-owner and cloud attenuation.
    pub(super) fn glare_frame(
        &self,
        camera: WorldCameraFrame,
        environment: RuntimeWorldEnvironmentFrame,
        resources: &crate::application::sky_resources::RuntimeCelestialResources,
        skybox_weight: f32,
    ) -> solarity_rendering::WorldGlareFrame {
        let [sun, moon, _] = self.celestials.bodies();
        solarity_rendering::WorldGlareFrame {
            camera,
            bodies: [sun, moon],
            textures: resources.glare_textures,
            colors: [resources.colors[0], resources.colors[1]],
            environment: solarity_rendering::WorldGlareEnvironment {
                day: environment.day_fraction(),
                elapsed_seconds: self.elapsed_seconds,
                cloud_alpha: [sun, moon].map(|body| {
                    self.clouds
                        .opacity_at(camera.camera().position(), body.position())
                }),
                liquid_depth: environment.camera_liquid_depth(),
                skybox_weight,
            },
        }
    }
}
