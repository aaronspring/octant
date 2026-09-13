use crate::app::OctantApp;
use crate::ui::settings::coastline::show_coastline_controls;

pub(crate) fn show_volume_options(app: &mut OctantApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Experimental")
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        let algo_label = match app.volume_algorithm {
            0 => "Volume Raymarching",
            1 => "Isosurface (Sobel)",
            2 => "Maximum Intensity (MIP)",
            3 => "Minimum Intensity (MinIP)",
            4 => "Average Projection (X-ray)",
            5 => "Categorical Label Surface",
            6 => "Absorption RGBA",
            7 => "Additive RGBA",
            8 => "Indexed RGBA",
            _ => "Shaded Contours",
        };
        ui.menu_button(egui::RichText::new(algo_label).small(), |ui| {
            let algos = [
                (0, "Volume Raymarching (DVR)"),
                (1, "Isosurface (Sub-Voxel + Sobel)"),
                (2, "Maximum Intensity (MIP)"),
                (3, "Minimum Intensity (MinIP)"),
                (4, "Average Projection (X-ray)"),
                (5, "Categorical Label Surface"),
                (6, "Absorption RGBA"),
                (7, "Additive RGBA"),
                (8, "Indexed RGBA"),
                (9, "Shaded Contours"),
            ];
            for (id, label) in algos {
                if ui
                    .selectable_label(app.volume_algorithm == id, label)
                    .clicked()
                {
                    app.volume_algorithm = id;
                    ui.close();
                }
            }
        });
    });

    ui.separator();
    ui.add(egui::Slider::new(&mut app.volume_step_count, 16..=256).text("Steps"));
    ui.checkbox(&mut app.volume_transparency, "Transparency");

    if app.volume_algorithm != 1
        && app.volume_algorithm != 2
        && app.volume_algorithm != 3
        && app.volume_algorithm != 4
        && app.volume_algorithm != 5
    {
        ui.add(egui::Slider::new(&mut app.volume_opacity, 0.1..=10.0).text("Density"));
    }

    if app.volume_algorithm == 2 {
        ui.add(egui::Slider::new(&mut app.volume_attenuation, 0.0..=5.0).text("Attenuation"));
    }

    if app.volume_algorithm == 0
        || app.volume_algorithm == 1
        || app.volume_algorithm == 2
        || app.volume_algorithm == 3
        || app.volume_algorithm == 4
        || app.volume_algorithm == 9
    {
        ui.separator();
        if app.volume_algorithm != 2 {
            ui.add(egui::Slider::new(&mut app.volume_cmin, 0.0..=100.0).text("Min Clip"));
        }
        ui.add(egui::Slider::new(&mut app.volume_cmax, 0.0..=100.0).text("Max Range"));
    }

    if app.volume_algorithm == 1 {
        ui.separator();
        ui.add(egui::Slider::new(&mut app.volume_isovalue, -100.0..=100.0).text("Isovalue"));
        ui.add(egui::Slider::new(&mut app.volume_isorange, 0.1..=20.0).text("Isorange"));
    }
}

pub(crate) fn show_sphere_options(app: &mut OctantApp, ui: &mut egui::Ui) {
    let modes: [(u32, &str); 4] = [(0, "Smooth"), (1, "Bumpy"), (2, "Steps"), (3, "Voxel")];

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (id, label) in modes {
            if ui
                .selectable_label(app.sphere_mode == id, egui::RichText::new(label))
                .clicked()
            {
                app.sphere_mode = id;
            }
        }
    });

    if app.sphere_mode > 0 {
        ui.separator();
        ui.add(egui::Slider::new(&mut app.sphere_displacement_strength, 0.0..=5.0).text("Height"));
    }

    show_coastline_controls(app, ui);
}

pub(crate) fn show_surface_options(app: &mut OctantApp, ui: &mut egui::Ui) {
    let modes: [(u32, &str); 3] = [(0, "Bumpy"), (1, "Steps"), (2, "Voxel")];

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (id, label) in modes {
            if ui
                .selectable_label(app.surface_mode == id, egui::RichText::new(label))
                .clicked()
            {
                app.surface_mode = id;
            }
        }
    });

    ui.separator();
    ui.add(egui::Slider::new(&mut app.surface_displacement_strength, 0.0..=5.0).text("Height"));

    show_coastline_controls(app, ui);
}

pub(crate) fn show_point_cloud_options(app: &mut OctantApp, ui: &mut egui::Ui) {
    ui.add(egui::Slider::new(&mut app.point_cloud_size, 0.002..=0.10).text("Size"));
}
