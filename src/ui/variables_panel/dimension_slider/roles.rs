//! Dimension spatial and animation role transitions.

use crate::app::{AnimationRole, OctantApp, SpatialRole};

pub fn apply_role_change(
    dim: usize,
    spatial: SpatialRole,
    anim: AnimationRole,
    app: &mut OctantApp,
) {
    let old_spatial = app.dim_config[dim].spatial;
    let old_anim = app.dim_config[dim].animation;

    if spatial != old_spatial && spatial != SpatialRole::None {
        for j in 0..app.dim_config.len() {
            if j != dim {
                let should_clear = (spatial == SpatialRole::Grid
                    && (app.dim_config[j].spatial == SpatialRole::Grid
                        || app.dim_config[j].spatial == SpatialRole::X
                        || app.dim_config[j].spatial == SpatialRole::Y))
                    || app.dim_config[j].spatial == spatial
                    || (app.dim_config[j].spatial == SpatialRole::Grid
                        && (spatial == SpatialRole::X || spatial == SpatialRole::Y));

                if should_clear {
                    app.dim_config[j].spatial = SpatialRole::None;
                    if app.dim_config[j].animation == AnimationRole::None {
                        app.dim_config[j].active = false;
                    }
                }
            }
        }
    }

    if anim != old_anim && anim == AnimationRole::Animated {
        for j in 0..app.dim_config.len() {
            if j != dim && app.dim_config[j].animation == AnimationRole::Animated {
                app.dim_config[j].animation = AnimationRole::None;
                if app.dim_config[j].spatial == SpatialRole::None {
                    app.dim_config[j].active = false;
                }
            }
        }
    }

    app.dim_config[dim].spatial = spatial;
    app.dim_config[dim].animation = anim;

    if spatial != SpatialRole::None || anim == AnimationRole::Animated {
        app.dim_config[dim].active = true;
    }

    if spatial != SpatialRole::None
        && let Some(dim_size) = app
            .active_dataset_metadata
            .as_ref()
            .and_then(|meta| meta.variables.get(app.selected_variable_idx))
            .and_then(|v_info| v_info.shape.get(dim).copied())
    {
        let dim_sz = dim_size as usize;
        if dim < app.selected_dim_ranges.len() {
            let (st, en) = app.selected_dim_ranges[dim];
            if st == en {
                app.selected_dim_ranges[dim] = (0, dim_sz.saturating_sub(1));
            }
        }
    }

    app.spatial_dims.clear();
    for j in 0..app.dim_config.len() {
        if app.dim_config[j].spatial != SpatialRole::None {
            app.spatial_dims.push(j);
        }
    }
    app.spatial_dims
        .sort_by_key(|&d| match app.dim_config[d].spatial {
            SpatialRole::Grid => 0,
            SpatialRole::X => 1,
            SpatialRole::Y => 2,
            SpatialRole::Z => 3,
            SpatialRole::None => 99,
        });

    app.animated_dim = app
        .dim_config
        .iter()
        .position(|c| c.animation == AnimationRole::Animated);
}
