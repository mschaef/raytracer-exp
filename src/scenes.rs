// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

use crate::scene_objects;

use crate::render::{
    Camera,
    Scene,
    Light,
    Surface,
};

use crate::render::color::{
    LinearColor,
};

use crate::render::shapes::{
    Sphere,
    Plane,
    Cuboid,
    Shape,
    group,
    translate,
    scale,
    rotate_x,
    rotate_y,
    rotate_z,
};

const REFLECT_LIMIT: u32 = 2;
const OVERSAMPLE: u32 = 2;

// `Camera::looking_at` performs a normalize and a couple of cross
// products, so it can't be a `const fn` for `f64`. A plain function
// returning a fresh Camera is the equivalent — cheap to call once per
// scene at construction time.
//
// `zoom = 1.0` corresponds to a vertical FOV of about 53°, which closely
// matches the previous fixed camera so existing scenes render at
// roughly the same framing.
fn default_camera() -> Camera {
    Camera::looking_at(
        [0.0, 10.0, 0.0],   // location
        [0.0,  0.0, 0.0],   // look at the origin
        [0.0,  0.0, 1.0],   // world up = +z
        1.0,                // zoom
    )
}

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
        camera: default_camera(),
        background: [0.0, 0.0, 0.0],
        lights: vec![Light::white([5.0, 5.0, 5.0])],
        objects: scene_objects![
            Sphere {
                center: [1.5, 2.0, 0.0],
                r: 0.7,
                surface: SURFACE_ORANGE
            },
            Sphere {
                center: [3.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_RED
            },
            Sphere {
                center: [-3.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_BLUE
            },
            Sphere {
                center: [0.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_GREEN
            },
            Sphere {
                center: [0.0, -4.0, 0.0],
                r: 3.0,
                surface: SURFACE_YELLOW
            },
            Sphere { // foreground sphere at back at list - proper occlusion required to make this visible
                center: [-1.5, 2.0, 0.0],
                r: 0.7,
                surface: SURFACE_PURPLE
            },
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
        camera: default_camera(),
        background: [0.0, 0.0, 0.0],
        lights: vec![Light::white([5.0, 5.0, 5.0])],
        objects: (0..25).map(| x | Shape::from(Sphere {
            center: [
                0.0 + ((x % 5) - 2) as f64,
                0.0,
                0.0 + ((x / 5) - 2) as f64,
            ],
            r: 0.4,
            surface: test_surface((x % 5) as f64 / 5.0, (x / 5) as f64 / 5.0)
        })).collect::<Vec<_>>(),
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

#[allow(dead_code)]
pub fn scene_one_sphere() -> Scene {
    Scene {
        name: "Single Sphere, Reflective Planes",
        camera: default_camera(),
        background: [0.0, 0.0, 0.0],
        lights: vec![Light::white([10.0, 10.0, 10.0])],
        objects: scene_objects![
            Sphere {
                center: [0.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_ORANGE
            },

            Plane {
                normal: [1.0, 0.0, 0.0],
                p0: [-2.0, 0.0, 0.0],
                surface: SURFACE_WHITE_C
            },
            Plane {
                normal: [0.0, 1.0, 0.0],
                p0: [0.0, -2.0, 0.0],
                surface: SURFACE_WHITE_C
            },
            Plane {
                normal: [0.0, 0.0, 1.0],
                p0: [0.0, 0.0, -2.0],
                surface: SURFACE_WHITE_C
            },
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

#[allow(dead_code)]
pub fn scene_axis_spheres() -> Scene {
    Scene {
        name: "Axis Spheres",
        camera: default_camera(),
        background: [0.0, 0.0, 0.0],
        lights: vec![Light::white([10.0, 10.0, 10.0])],
        objects: scene_objects![
            Sphere {
                center: [0.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_WHITE
            },
            Sphere {
                center: [3.0, 0.0, 0.0],
                r: 0.25,
                surface: SURFACE_RED
            },
            Sphere {
                center: [0.0, 3.0, 0.0],
                r: 0.25,
                surface: SURFACE_GREEN
            },
            Sphere {
                center: [0.0, 0.0, 3.0],
                r: 0.25,
                surface: SURFACE_BLUE
            },
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}


#[allow(dead_code)]
pub fn scene_cuboid_test() -> Scene {
    Scene {
        name: "Cuboid Test",
        camera: default_camera(),
        background: [0.0, 0.0, 0.0],
        lights: vec![Light::white([10.0, 10.0, 10.0])],
        objects: scene_objects![
            // A tall narrow red box at the origin.
            Cuboid {
                center: [0.0, 0.0, 0.0],
                size: [1.5, 1.5, 2.5],
                surface: SURFACE_RED
            },
            // A small green cube floating to the right.
            Cuboid {
                center: [3.0, 0.0, 0.5],
                size: [1.0, 1.0, 1.0],
                surface: SURFACE_GREEN
            },
            // A wide flat blue slab on the left.
            Cuboid {
                center: [-2.5, 0.5, -0.75],
                size: [1.5, 2.0, 0.5],
                surface: SURFACE_BLUE
            },
            // A sphere for visual reference and to confirm interaction with
            // existing primitives still works.
            Sphere {
                center: [1.0, -2.5, 0.5],
                r: 0.6,
                surface: SURFACE_YELLOW
            },
            // A reflective checkered ground plane to catch shadows.
            Plane {
                normal: [0.0, 0.0, 1.0],
                p0: [0.0, 0.0, -2.0],
                surface: SURFACE_WHITE_C
            },
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

#[allow(dead_code)]
pub fn scene_group_test() -> Scene {
    // Demonstrates `Shape::Group` as a hierarchical container. Visually
    // this looks the same as a flat scene with the same primitives —
    // grouping has no rendering effect on its own, but it sets up the
    // structure that transforms will hang off of in the next step.
    Scene {
        name: "Group Test",
        camera: default_camera(),
        background: [0.0, 0.0, 0.0],
        lights: vec![Light::white([10.0, 10.0, 10.0])],
        objects: scene_objects![
            // A "snowman": three stacked spheres treated as a single child
            // of the scene. `scene_objects!` builds a Vec<Shape> which
            // `group(...)` then wraps as Shape::Group — the macro and the
            // constructor compose without any extra plumbing.
            group(scene_objects![
                Sphere { center: [-2.0, 0.0, -1.0], r: 0.6, surface: SURFACE_WHITE },
                Sphere { center: [-2.0, 0.0,  0.0], r: 0.5, surface: SURFACE_WHITE },
                Sphere { center: [-2.0, 0.0,  0.8], r: 0.4, surface: SURFACE_WHITE },
            ]),
            // A row of three cubes, also grouped, to confirm the same
            // mechanism works for boxes and that nearest-hit is correct
            // when groups contain different primitive types.
            group(scene_objects![
                Cuboid { center: [1.0, 0.0, -1.0], size: [0.6, 0.6, 0.6], surface: SURFACE_RED    },
                Cuboid { center: [2.0, 0.0, -1.0], size: [0.6, 0.6, 0.6], surface: SURFACE_GREEN  },
                Cuboid { center: [3.0, 0.0, -1.0], size: [0.6, 0.6, 0.6], surface: SURFACE_BLUE   },
            ]),
            // A nested group: a sphere alongside an inner group of two
            // smaller spheres. Confirms recursion through Group::hit_test
            // works to arbitrary depth.
            group(scene_objects![
                Sphere { center: [0.0, -3.0, -1.0], r: 0.7, surface: SURFACE_PURPLE },
                group(scene_objects![
                    Sphere { center: [-0.7, -3.0, 0.2], r: 0.3, surface: SURFACE_ORANGE },
                    Sphere { center: [ 0.7, -3.0, 0.2], r: 0.3, surface: SURFACE_YELLOW },
                ]),
            ]),
            // A ground plane outside any group, to confirm flat and
            // grouped objects coexist correctly in the same scene.
            Plane {
                normal: [0.0, 0.0, 1.0],
                p0: [0.0, 0.0, -2.0],
                surface: SURFACE_WHITE_C
            },
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

#[allow(dead_code)]
pub fn scene_transform_test() -> Scene {
    // Each object exercises a different transform path. Because the
    // checker pattern on the ground keys off world-space coordinates,
    // shadows and reflections cast by these transformed objects should
    // line up with the world-space silhouette of the *transformed*
    // shape, which is the visual proof that the math is right.
    use std::f64::consts::PI;

    Scene {
        name: "Transform Test",
        camera: default_camera(),
        background: [0.0, 0.0, 0.0],
        lights: vec![Light::white([10.0, 10.0, 10.0])],
        objects: scene_objects![
            // A unit cube translated to (3, 0, 0). Should look identical
            // to a Cuboid declared with center=[3,0,0] directly.
            translate([3.0, 0.0, 0.0],
                Cuboid {
                    center: [0.0, 0.0, 0.0],
                    size: [1.0, 1.0, 1.0],
                    surface: SURFACE_RED
                }),

            // A unit cube rotated 30 degrees around z, then translated.
            // The face shading should clearly show the rotation: edges
            // and corners of the cube no longer line up with world axes.
            translate([-3.0, 0.0, 0.0],
                rotate_z(PI / 6.0,
                    Cuboid {
                        center: [0.0, 0.0, 0.0],
                        size: [1.5, 1.5, 1.5],
                        surface: SURFACE_GREEN
                    })),

            // A unit sphere stretched non-uniformly into an ellipsoid,
            // then translated. The point of this case is to verify the
            // inverse-transpose normal handling: if normals were
            // transformed with the forward matrix instead, the lighting
            // on the long axis would be visibly wrong.
            translate([0.0, 3.0, 0.0],
                scale([1.5, 0.6, 0.6],
                    Sphere {
                        center: [0.0, 0.0, 0.0],
                        r: 1.0,
                        surface: SURFACE_BLUE
                    })),

            // A grouped pair of spheres, then transformed as a unit.
            // Confirms that Group nests correctly inside Transform —
            // which is the whole point of having both kinds of nodes:
            // composite objects can be transformed with one wrapper.
            translate([0.0, -3.0, 0.5],
                rotate_y(PI / 4.0,
                    group(scene_objects![
                        Sphere {
                            center: [-0.7, 0.0, 0.0],
                            r: 0.4,
                            surface: SURFACE_ORANGE
                        },
                        Sphere {
                            center: [ 0.7, 0.0, 0.0],
                            r: 0.4,
                            surface: SURFACE_YELLOW
                        },
                    ]))),

            // Nested transforms: outer translate, inner rotate around x,
            // and a deeper rotation around z. Each level inverse-
            // transforms the ray as it descends, so the leaf sees the
            // composed inverse of all three. Visually: a cube tilted in
            // two axes, then placed off-origin.
            translate([0.0, 0.0, 1.5],
                rotate_x(PI / 5.0,
                    rotate_z(PI / 7.0,
                        Cuboid {
                            center: [0.0, 0.0, 0.0],
                            size: [0.8, 0.8, 0.8],
                            surface: SURFACE_PURPLE
                        }))),

            // Reflective checkered ground plane, untransformed. Catches
            // shadows from all the transformed objects above.
            Plane {
                normal: [0.0, 0.0, 1.0],
                p0: [0.0, 0.0, -2.0],
                surface: SURFACE_WHITE_C
            },
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

#[allow(dead_code)]
pub fn scene_multi_light_test() -> Scene {
    // Two colored point lights flanking a white sphere. The center
    // sphere should show a clear red-on-the-left, blue-on-the-right
    // gradient with a magenta band where both lights reach. The smaller
    // accent spheres confirm the contributions still sum correctly when
    // multiple objects are present, and the ground catches color-tinted
    // shadows from each light cast in opposite directions.
    Scene {
        name: "Multi-Light Test",
        camera: default_camera(),
        background: [0.0, 0.0, 0.0],
        lights: vec![
            // Red light coming from image-left, slightly behind the camera.
            Light::point([-5.0, 5.0, 5.0], [1.0, 0.2, 0.2], 1.0),
            // Blue light coming from image-right, lower intensity so the
            // asymmetry between the two contributions is visible.
            Light::point([ 5.0, 5.0, 5.0], [0.2, 0.4, 1.0], 0.7),
        ],
        objects: scene_objects![
            // Central white sphere — picks up whatever color the lights
            // throw at it without bias.
            Sphere {
                center: [0.0, 0.0, 0.0],
                r: 1.0,
                surface: SURFACE_WHITE
            },
            // Smaller accent spheres for visual reference and to confirm
            // shadows from one occluder don't affect another.
            Sphere {
                center: [-2.5, 0.0, -0.5],
                r: 0.4,
                surface: SURFACE_WHITE
            },
            Sphere {
                center: [ 2.5, 0.0, -0.5],
                r: 0.4,
                surface: SURFACE_WHITE
            },
            // Reflective checkered ground plane.
            Plane {
                normal: [0.0, 0.0, 1.0],
                p0: [0.0, 0.0, -1.5],
                surface: SURFACE_WHITE_C
            },
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

#[allow(dead_code)]
pub fn scene_ball_on_plane() -> Scene {
    Scene {
        name: "Ball on Plane",
        camera: default_camera(),
        background: [0.0, 0.0, 0.0],
        lights: vec![Light::white([10.0, 10.0, 10.0])],
        objects: scene_objects![
            Sphere {
                center: [0.0, -2.0, -1.0],
                r: 0.66,
                surface: SURFACE_BLUE
            },
            Plane {
                normal: [0.0, 0.0, 1.0],
                p0: [0.0, 0.0, -2.0],
                surface: SURFACE_WHITE_C
            },
        ],
        reflect_limit: REFLECT_LIMIT,
        oversample: OVERSAMPLE,
    }
}

