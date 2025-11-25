use bevy::prelude::*;
use bevy_egui::*;
use bevy_meshopt::*;

pub fn main() -> AppExit {
    App::new()
        .insert_resource(HelmetEntity(None))
        .insert_resource(Reset(true))
        .insert_resource(Simplify(false))
        .insert_resource(SimplifySettings(default()))
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin::default())
        .add_plugins(bevy_inspector_egui::quick::WorldInspectorPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Startup, load_gltf)
        .add_systems(Update, (reset_gltf_object, simplify_meshes).chain())
        .add_systems(EguiPrimaryContextPass, simplify_settings_ui)
        .run()
}

// Holds the scene handle
#[derive(Resource)]
struct HelmetScene(Handle<Gltf>);

#[derive(Resource, Default)]
struct HelmetEntity(Option<Entity>);

fn load_gltf(mut commands: Commands, asset_server: Res<AssetServer>) {
    let gltf = asset_server.load("models/FlightHelmet/FlightHelmet.gltf");
    commands.insert_resource(HelmetScene(gltf));
}

#[derive(Resource)]
pub struct Reset(bool);

fn reset_gltf_object(
    mut reset: ResMut<Reset>,
    mut commands: Commands,
    helmet_scene: Res<HelmetScene>,
    mut helmet_entity: ResMut<HelmetEntity>,
    gltf_assets: Res<Assets<Gltf>>,
) {
    if !reset.0 {
        return;
    }

    if let Some(helmet_entity) = helmet_entity.0 {
        commands.entity(helmet_entity).despawn();
    }

    helmet_entity.0 = None;
    if let Some(gltf) = gltf_assets.get(&helmet_scene.0) {
        let new_id = commands.spawn(SceneRoot(gltf.scenes[0].clone())).id();
        helmet_entity.0 = Some(new_id);
    }

    // // Spawns the scene named "Lenses_low"
    // commands.spawn((
    //     SceneRoot(gltf.named_scenes["Lenses_low"].clone()),
    //     Transform::from_xyz(1.0, 2.0, 3.0),
    // ));
    //
    reset.0 = false;
}

pub fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.7, 0.7, 1.0).looking_at(Vec3::new(0.0, 0.3, 0.0), Vec3::Y),
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
            specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
            intensity: 250.0,
            ..default()
        },
    ));
}

#[derive(Resource)]
pub struct Simplify(bool);

fn simplify_meshes(
    mut simplify: ResMut<Simplify>,
    params: Res<SimplifySettings>,
    query: Query<&Mesh3d>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !simplify.0 {
        return;
    }
    info!("simplify params: {:?}", params.0);

    let mut positions_before = 0;
    let mut positions_after = 0;
    let mut indices_before = 0;
    let mut indices_after = 0;

    for mesh in query.iter() {
        if let Some(mesh) = meshes.get_mut(mesh.id()) {
            positions_before += mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .map_or(0, |a| a.len());
            indices_before += mesh.indices().map_or(0, |indices| indices.len());

            mesh.assert_indices_u32();
            if let Err(err) = mesh.simplify_in_place(&params.0) {
                error!("Mesh simplification failed: {}", err);
            };

            positions_after += mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .map_or(0, |a| a.len());
            indices_after += mesh.indices().map_or(0, |indices| indices.len());
        }
    }

    info!(
        "Positions before: {}, after: {}",
        positions_before, positions_after
    );
    info!(
        "Indices before: {}, after: {}",
        indices_before, indices_after
    );

    simplify.0 = false;
}

#[derive(Resource, Deref, DerefMut)]
pub struct SimplifySettings(SimplifyParams<'static>);

// UI system
pub fn simplify_settings_ui(
    mut contexts: EguiContexts,
    mut settings: ResMut<SimplifySettings>,
    mut reset: ResMut<Reset>,
    mut simplify: ResMut<Simplify>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("Simplify")
        .default_width(300.0)
        .show(ctx, |ui| {
            // Max Error
            egui::Grid::new("Property grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Max Error:");
                    ui.add(
                        egui::Slider::new(&mut settings.max_error, 0.0..=1.0)
                            .logarithmic(true)
                            .text("error"),
                    );

                    ui.end_row();

                    ui.label("Target Count:");
                    let mut target_type = match settings.target_index_count {
                        TargetIndices::Count(_) => "Count",
                        TargetIndices::Multiplier(_) => "Multiplier",
                    };

                    egui::ComboBox::from_label("")
                        .selected_text(target_type)
                        .show_ui(ui, |ui| {
                            if ui.selectable_value(&mut target_type, "Count", "Count").clicked() {
                                if !matches!(settings.target_index_count, TargetIndices::Count(_)) {
                                    settings.target_index_count = TargetIndices::Count(1000);
                                }
                            }

                            if ui.selectable_value(&mut target_type, "Multiplier", "Multiplier").clicked() {
                                if !matches!(settings.target_index_count, TargetIndices::Multiplier(_)) {
                                    settings.target_index_count = TargetIndices::Multiplier(0.5);
                                }
                            }
                        });

                    ui.end_row();
                    ui.label("");
                    match &mut settings.target_index_count {
                        TargetIndices::Count(count) => {
                            ui.add(
                                egui::Slider::new(count, 1..=100000)
                                    .logarithmic(true)
                                    .text("triangles"),
                            );
                        }
                        TargetIndices::Multiplier(multiplier) => {
                            ui.add(egui::Slider::new(multiplier, 0.0..=1.0).text("%"));
                        }
                    }
                });


            // egui::ComboBox::from_label("Target Count")
            //     .selected_text(format!("{:?}", settings.target_index_count))
            //     .show_ui(ui, |ui| {
            //         // ui.add(egui::Slider::new(&mut settings.target_count, 1..=1000000));
            //     });

            ui.add_space(10.0);

            // Simplify Options (bitset)
            ui.label("Options:");
            if ui.checkbox(
                &mut settings.options.contains(SimplifyOptions::LockBorder),
                "Lock Border",
            )
            .on_hover_text("Prevent border vertices from moving").clicked() {
                settings.options.toggle(SimplifyOptions::LockBorder);
            };

            if ui.checkbox(
                &mut settings.options.contains(SimplifyOptions::Sparse),
                "Sparse",
            )
            .on_hover_text("Use sparse decimation").clicked() {
                settings.options.toggle(SimplifyOptions::Sparse);
            };

            if ui.checkbox(
                &mut settings.options.contains(SimplifyOptions::ErrorAbsolute),
                "Error Absolute",
            )
            .on_hover_text("Use absolute error instead of relative").clicked() {
                settings.options.toggle(SimplifyOptions::ErrorAbsolute);
            };

            if ui.checkbox(
                &mut settings.options.contains(SimplifyOptions::Regularize),
                "Regularize",
            )
            .on_hover_text("Produce more regular triangle sizes and shapes during simplification, at some cost to geometric quality")
            .clicked() {
                settings.options.toggle(SimplifyOptions::Regularize);
            }

            // Sloppy
            ui.checkbox(&mut settings.sloppy, "Sloppy")
                .on_hover_text("Use faster but less accurate simplification");

            ui.add_space(10.0);

            // let mut is_percentage = matches!(settings.target_count, TargetCount::Percentage(_));
            // ui.horizontal(|ui| {
            //     if ui.radio(!is_percentage, "Count").clicked() {
            //         settings.target_count = TargetCount::Count(1000);
            //     }
            //     if ui.radio(is_percentage, "Percentage").clicked() {
            //         settings.target_count = TargetCount::Percentage(0.5);
            //     }
            // });


            ui.add_space(10.0);

            if ui.button("Reset").clicked() {
                reset.0 = true;
            }
            if ui.button("Simplify").clicked() {
                simplify.0 = true;
            }

            // Display current settings
            ui.separator();
            ui.collapsing("Current Settings", |ui| {
                ui.label(format!("Max Error: {:.4}", settings.max_error));
                ui.label(format!("Options: {:?}", settings.options));
                ui.label(format!("Sloppy: {}", settings.sloppy));
                // ui.label(format!("Target: {:?}", settings.target_count));
            });
        });
}
