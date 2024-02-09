// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

use crate::render::{
    Camera,
    Color,
    Scene,
    Light,
    Surface,
    Sphere,
    Plane,
};

const DEFAULT_CAMERA: Camera = Camera {
    location: [0.0, 10.0, 0.0],
    point_at: [0.0, 0.0, 0.0],
    u: [10.0, 0.0, 0.0],
    v: [0.0, 0.0, -10.0]
};

#[allow(dead_code)]
const AMBIENT: f64 = 0.2 as f64;

#[allow(dead_code)]
const SPECULAR: f64 = 0.5 as f64;

#[allow(dead_code)]
const LIGHT: f64 = 0.6 as f64;

#[allow(dead_code)]
const REFLECTION: f64 = 0.5 as f64;

#[allow(dead_code)]
const fn surface_glossy(c: Color) -> Surface {
    Surface {
        color: c,
        ambient: 0.2,
        specular: 0.5,
        light: LIGHT,
        checked: false,
        reflection: 0.0
    }
}

#[allow(dead_code)]
const fn reflective(s: Surface) -> Surface {
    Surface {
        reflection: 0.2,
        .. s
    }
}

#[allow(dead_code)]
pub const SURFACE_RED: Surface = surface_glossy([1.0, 0.0, 0.0]);

#[allow(dead_code)]
pub const SURFACE_GREEN: Surface = surface_glossy([0.0, 1.0, 0.0]);

#[allow(dead_code)]
pub const SURFACE_BLUE: Surface = surface_glossy([0.0, 0.0, 1.0]);

#[allow(dead_code)]
pub const SURFACE_PURPLE: Surface = surface_glossy([1.0, 0.0, 1.0]);

#[allow(dead_code)]
pub const SURFACE_ORANGE: Surface = surface_glossy([1.0, 0.5, 0.0]);

#[allow(dead_code)]
pub const SURFACE_YELLOW: Surface = surface_glossy([1.0, 1.0, 0.0]);

#[allow(dead_code)]
pub const SURFACE_WHITE: Surface = surface_glossy([1.0, 1.0, 1.0]);

#[allow(dead_code)]
pub const SURFACE_BLACK: Surface = surface_glossy([0.0, 0.0, 0.0]);

#[allow(dead_code)]
pub const SURFACE_WHITE_C: Surface = Surface {
    color: [0.2, 0.2, 0.2],
    ambient: AMBIENT,
    specular: SPECULAR,
    light: LIGHT,
    checked: true,
    reflection: 0.5
};

#[allow(dead_code)]
pub fn scene_one_sphere(sphere_surface: Surface) -> Scene {
    Scene {
        name: "Single Sphere",
        camera: DEFAULT_CAMERA,
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: [10.0, 10.0, 10.0]
        },
        objects: vec![
            Box::new(Sphere {
                center: [0.0, 0.0, 0.0],
                r: 3.0,
                surface: sphere_surface
            }),
        ]
    }
}

