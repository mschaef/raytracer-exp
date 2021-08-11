use crate::render::{
    Camera,
    Scene,
    Light,
    Hittable,
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

const AMBIENT: f64 = 0.2 as f64;
const SPECULAR: f64 = 0.5 as f64;
const LIGHT: f64 = 0.6 as f64;


const SURFACE_RED: Surface = Surface {
    color: [1.0, 0.0, 0.0],
    ambient: AMBIENT,
    specular: SPECULAR,
    light: LIGHT
};

const SURFACE_GREEN: Surface = Surface {
    color: [0.0, 1.0, 0.0],
    ambient: AMBIENT,
    specular: SPECULAR,
    light: LIGHT
};

const SURFACE_BLUE: Surface = Surface {
    color: [0.0, 0.0, 1.0],
    ambient: AMBIENT,
    specular: SPECULAR,
    light: LIGHT
};

const SURFACE_ORANGE: Surface = Surface {
    color: [1.0, 0.5, 0.0],
    ambient: AMBIENT,
    specular: SPECULAR,
    light: LIGHT
};

const SURFACE_YELLOW: Surface = Surface {
    color: [1.0, 1.0, 0.0],
    ambient: AMBIENT,
    specular: SPECULAR,
    light: LIGHT
};

const SURFACE_PURPLE: Surface = Surface {
    color: [1.0, 0.0, 1.0],
    ambient: AMBIENT,
    specular: SPECULAR,
    light: LIGHT
};

const SURFACE_WHITE: Surface = Surface {
    color: [1.0, 1.0, 1.0],
    ambient: AMBIENT,
    specular: SPECULAR,
    light: LIGHT
};

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
        ]
    }
}

fn test_surface(light: f64, specular: f64) -> Surface {
    Surface {
        color: [1.0, 0.0, 0.0],
        ambient: AMBIENT,
        specular: specular,
        light: light
    }
}


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
        }) as Box<dyn Hittable>).collect::<Vec<_>>()
    }
}

pub fn scene_one_sphere() -> Scene {
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
                r: 1.0,
                surface: SURFACE_ORANGE
            }),

            Box::new(Plane {
                normal: [1.0, 0.0, 0.0],
                p0: [-3.0, 0.0, 0.0],
                surface: SURFACE_RED
            }),
            Box::new(Plane {
                normal: [0.0, 1.0, 0.0],
                p0: [0.0, -3.0, 0.0],
                surface: SURFACE_GREEN
            }),
            Box::new(Plane {
                normal: [0.0, 0.0, 1.0],
                p0: [0.0, 0.0, -3.0],
                surface: SURFACE_BLUE
            }),
        ]
    }
}

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
        ]
    }
}


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
                surface: SURFACE_WHITE
            }),
        ]
    }
}

