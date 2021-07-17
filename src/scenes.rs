use crate::render::{
    Scene,
    Light,
    Sphere,
    Plane,
};

pub fn scene_sphere_occlusion_test() -> Scene {
    Scene {
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (5.0, 5.0, 5.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (1.5, 2.0, 0.0),
                r: 0.7,
                color: [1.0, 0.5, 0.0]
            }),
            Box::new(Sphere {
                center: (3.0, 0.0, 0.0),
                r: 1.0,
                color: [1.0, 0.0, 0.0]
            }),
            Box::new(Sphere {
                center: (-3.0, 0.0, 0.0),
                r: 1.0,
                color: [0.0, 0.0, 1.0],
            }),
            Box::new(Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                color: [0.0, 1.0, 0.0]
            }),
            Box::new(Sphere {
                center: (0.0, -4.0, 0.0),
                r: 3.0,
                color: [1.0, 1.0, 0.0]
            }),
            Box::new(Sphere { // foreground sphere at back at list - proper occlusion required to make this visible
                center: (-1.5, 2.0, 0.0),
                r: 0.7,
                color: [1.0, 0.0, 1.0]
            }),
        ]
    }
}

pub fn scene_one_sphere() -> Scene {
    Scene {
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                color: [1.0, 0.5, 0.0]
            }),

            Box::new(Plane {
                normal: (1.0, 0.0, 0.0),
                p0: (-3.0, 0.0, 0.0),
                color: [1.0, 0.0, 0.0]
            }),
            Box::new(Plane {
                normal: (0.0, 1.0, 0.0),
                p0: (0.0, -3.0, 0.0),
                color: [0.0, 1.0, 0.0]
            }),
            Box::new(Plane {
                normal: (0.0, 0.0, 1.0),
                p0: (0.0, 0.0, -3.0),
                color: [0.0, 0.0, 1.0]
            }),
        ]
    }
}

pub fn scene_axis_spheres() -> Scene {
    Scene {
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                color: [1.0, 1.0, 1.0]
            }),
            Box::new(Sphere {
                center: (3.0, 0.0, 0.0),
                r: 0.25,
                color: [1.0, 0.0, 0.0]
            }),
            Box::new(Sphere {
                center: (0.0, 3.0, 0.0),
                r: 0.25,
                color: [0.0, 1.0, 0.0]
            }),
            Box::new(Sphere {
                center: (0.0, 0.0, 3.0),
                r: 0.25,
                color: [0.0, 0.0, 1.0]
            }),
        ]
    }
}


pub fn scene_ball_on_plane() -> Scene {
    Scene {
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (0.0, -2.0, -1.0),
                r: 0.66,
                color: [0.0, 0.0, 1.0]
            }),
            Box::new(Plane {
                normal: (0.0, 0.0, 1.0),
                p0: (0.0, 0.0, -2.0),
                color: [1.0, 1.0, 1.0]
            }),

        ]
    }
}

