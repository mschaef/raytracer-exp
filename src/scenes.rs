use crate::render::{
    Camera,
    Scene,
    Light,
    Surface,
    Sphere,
    Plane,
};

const DEFAULT_CAMERA: Camera = Camera {
    location: (0.0, 10.0, 0.0),
    point_at: (0.0, 0.0, 0.0),
    u: (10.0, 0.0, 0.0),
    v: (0.0, 0.0, -10.0)
};

pub fn scene_sphere_occlusion_test() -> Scene {
    Scene {
        camera: DEFAULT_CAMERA,
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (5.0, 5.0, 5.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (1.5, 2.0, 0.0),
                r: 0.7,
                surface: Surface {
                    color: [1.0, 0.5, 0.0]
                }
            }),
            Box::new(Sphere {
                center: (3.0, 0.0, 0.0),
                r: 1.0,
                surface: Surface {
                    color: [1.0, 0.0, 0.0]
                }
            }),
            Box::new(Sphere {
                center: (-3.0, 0.0, 0.0),
                r: 1.0,
                surface: Surface {
                    color: [0.0, 0.0, 1.0],
                }
            }),
            Box::new(Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                surface: Surface {
                    color: [0.0, 1.0, 0.0]
                }
            }),
            Box::new(Sphere {
                center: (0.0, -4.0, 0.0),
                r: 3.0,
                surface: Surface {
                    color: [1.0, 1.0, 0.0]
                }
            }),
            Box::new(Sphere { // foreground sphere at back at list - proper occlusion required to make this visible
                center: (-1.5, 2.0, 0.0),
                r: 0.7,
                surface: Surface {
                    color: [1.0, 0.0, 1.0]
                }
            }),
        ]
    }
}

pub fn scene_one_sphere() -> Scene {
    Scene {
        camera: DEFAULT_CAMERA,
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                surface: Surface {
                    color: [1.0, 0.5, 0.0]
                }
            }),

            Box::new(Plane {
                normal: (1.0, 0.0, 0.0),
                p0: (-3.0, 0.0, 0.0),
                surface: Surface {
                    color: [1.0, 0.0, 0.0]
                }
            }),
            Box::new(Plane {
                normal: (0.0, 1.0, 0.0),
                p0: (0.0, -3.0, 0.0),
                surface: Surface {
                    color: [0.0, 1.0, 0.0]
                }
            }),
            Box::new(Plane {
                normal: (0.0, 0.0, 1.0),
                p0: (0.0, 0.0, -3.0),
                surface: Surface {
                    color: [0.0, 0.0, 1.0]
                }
            }),
        ]
    }
}

pub fn scene_axis_spheres() -> Scene {
    Scene {
        camera: DEFAULT_CAMERA,
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                surface: Surface {
                    color: [1.0, 1.0, 1.0]
                }
            }),
            Box::new(Sphere {
                center: (3.0, 0.0, 0.0),
                r: 0.25,
                surface: Surface {
                    color: [1.0, 0.0, 0.0]
                }
            }),
            Box::new(Sphere {
                center: (0.0, 3.0, 0.0),
                r: 0.25,
                surface: Surface {
                    color: [0.0, 1.0, 0.0]
                }
            }),
            Box::new(Sphere {
                center: (0.0, 0.0, 3.0),
                r: 0.25,
                surface: Surface {
                    color: [0.0, 0.0, 1.0]
                }
            }),
        ]
    }
}


pub fn scene_ball_on_plane() -> Scene {
    Scene {
        camera: DEFAULT_CAMERA,
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (0.0, -2.0, -1.0),
                r: 0.66,
                surface: Surface {
                    color: [0.0, 0.0, 1.0]
                }
            }),
            Box::new(Plane {
                normal: (0.0, 0.0, 1.0),
                p0: (0.0, 0.0, -2.0),
                surface: Surface {
                    color: [1.0, 1.0, 1.0]
                }
            }),
        ]
    }
}

