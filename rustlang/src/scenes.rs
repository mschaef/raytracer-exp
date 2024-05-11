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
    Scene,
    Light,
    Hittable,
    Surface,
};

use crate::render::color::{
    LinearColor,
};

use crate::render::shapes::{
    Sphere,
    Plane,
};

const REFLECT_LIMIT: u32 = 2;
const OVERSAMPLE: u32 = 2;

const DEFAULT_CAMERA: Camera = Camera {
    location: [0.0, 10.0, 0.0],
    point_at: [0.0, 0.0, 0.0],
    u: [10.0, 0.0, 0.0],
    v: [0.0, 0.0, -10.0]
};

#[allow(dead_code)]
const AMBIENT: f64 = 0.2_f64;

#[allow(dead_code)]
const SPECULAR: f64 = 0.5_f64;

#[allow(dead_code)]
const LIGHT: f64 = 0.6_f64;

#[allow(dead_code)]
const REFLECTION: f64 = 0.5_f64;

#[allow(dead_code)]
const fn surface_glossy(c: LinearColor) -> Surface {
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
const SURFACE_RED: Surface = surface_glossy([1.0, 0.0, 0.0]);

#[allow(dead_code)]
const SURFACE_GREEN: Surface = surface_glossy([0.0, 1.0, 0.0]);

#[allow(dead_code)]
const SURFACE_BLUE: Surface = surface_glossy([0.0, 0.0, 1.0]);

#[allow(dead_code)]
const SURFACE_PURPLE: Surface = surface_glossy([1.0, 0.0, 1.0]);

#[allow(dead_code)]
const SURFACE_ORANGE: Surface = surface_glossy([1.0, 0.5, 0.0]);

#[allow(dead_code)]
const SURFACE_YELLOW: Surface = surface_glossy([1.0, 1.0, 0.0]);

#[allow(dead_code)]
const SURFACE_WHITE: Surface = surface_glossy([1.0, 1.0, 1.0]);

#[allow(dead_code)]
const SURFACE_BLACK: Surface = surface_glossy([0.0, 0.0, 0.0]);

#[allow(dead_code)]
const SURFACE_WHITE_C: Surface = Surface {
    color: [0.2, 0.2, 0.2],
    ambient: AMBIENT,
    specular: SPECULAR,
    light: LIGHT,
    checked: true,
    reflection: 0.5
};

#[allow(dead_code)]
pub fn scene_sphere_occlusion_test() -> Scene {
    Scene {
        name: "Occlusion Test",
        camera: DEFAULT_CAMERA,
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: [5.0, 5.0, 5.0]
        },
        objects: vec![
            Box::new(Sphere {
                center: [1.5, 2.0, 0.0],
                r: 0.7,
                surface: SURFACE_ORANGE
            }),
            Box::new(Sphere {
                center: [3.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_RED
            }),
            Box::new(Sphere {
                center: [-3.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_BLUE
            }),
            Box::new(Sphere {
                center: [0.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_GREEN
            }),
            Box::new(Sphere {
                center: [0.0, -4.0, 0.0],
                r: 3.0,
                surface: SURFACE_YELLOW
            }),
            Box::new(Sphere { // foreground sphere at back at list - proper occlusion required to make this visible
                center: [-1.5, 2.0, 0.0],
                r: 0.7,
                surface: SURFACE_PURPLE
            }),
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

#[allow(dead_code)]
fn test_surface(light: f64, specular: f64) -> Surface {
    Surface {
        color: [1.0, 0.0, 0.0],
        ambient: AMBIENT,
        specular,
        light,
        checked: false,
        reflection: 0.0
    }
}


#[allow(dead_code)]
pub fn scene_sphere_surface_test() -> Scene {
    Scene {
        name: "Surface Finish Test",
        camera: DEFAULT_CAMERA,
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: [5.0, 5.0, 5.0]
        },
        objects: (0..25).map(| x | Box::new(Sphere {
            center: [
                0.0 + ((x % 5) - 2) as f64,
                0.0,
                0.0 + ((x / 5) - 2) as f64,
            ],
            r: 0.4,
            surface: test_surface((x % 5) as f64 / 5.0, (x / 5) as f64 / 5.0)
        }) as Box<dyn Hittable + Send + Sync>).collect::<Vec<_>>(),
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

#[allow(dead_code)]
pub fn scene_one_sphere() -> Scene {
    Scene {
        name: "Single Sphere, Reflective Planes",
        camera: DEFAULT_CAMERA,
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: [10.0, 10.0, 10.0]
        },
        objects: vec![
            Box::new(Sphere {
                center: [0.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_ORANGE
            }),

            Box::new(Plane {
                normal: [1.0, 0.0, 0.0],
                p0: [-2.0, 0.0, 0.0],
                surface: SURFACE_WHITE_C
            }),
            Box::new(Plane {
                normal: [0.0, 1.0, 0.0],
                p0: [0.0, -2.0, 0.0],
                surface: SURFACE_WHITE_C
            }),
            Box::new(Plane {
                normal: [0.0, 0.0, 1.0],
                p0: [0.0, 0.0, -2.0],
                surface: SURFACE_WHITE_C
            }),
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

#[allow(dead_code)]
pub fn scene_axis_spheres() -> Scene {
    Scene {
        name: "Axis Spheres",
        camera: DEFAULT_CAMERA,
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: [10.0, 10.0, 10.0]
        },
        objects: vec![
            Box::new(Sphere {
                center: [0.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_WHITE
            }),
            Box::new(Sphere {
                center: [3.0, 0.0, 0.0],
                r: 0.25,
                surface: SURFACE_RED
            }),
            Box::new(Sphere {
                center: [0.0, 3.0, 0.0],
                r: 0.25,
                surface: SURFACE_GREEN
            }),
            Box::new(Sphere {
                center: [0.0, 0.0, 3.0],
                r: 0.25,
                surface: SURFACE_BLUE
            }),
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}


#[allow(dead_code)]
pub fn scene_ball_on_plane() -> Scene {
    Scene {
        name: "Ball on Plane",
        camera: DEFAULT_CAMERA,
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: [10.0, 10.0, 10.0]
        },
        objects: vec![
            Box::new(Sphere {
                center: [0.0, -2.0, -1.0],
                r: 0.66,
                surface: SURFACE_BLUE
            }),
            Box::new(Plane {
                normal: [0.0, 0.0, 1.0],
                p0: [0.0, 0.0, -2.0],
                surface: SURFACE_WHITE_C
            }),
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

