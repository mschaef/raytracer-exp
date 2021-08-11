pub type Point = [f64; 3];

pub struct Vector {
    pub start: Point,
    pub delta: Point
}

pub const EPSILON: f64 = 0.0001;

pub fn lenp(pt: Point) -> f64 {
    let [x, y, z] = pt;

    ((x * x) + (y * y) + (z * z)).sqrt()
}

pub fn normalizep(pt: Point) -> Point {
    let [x, y, z] = pt;

    let len = lenp(pt);

    if len < EPSILON {
        panic!("Cannot normalize point vector of length 0.");
    }

    [x / len, y / len, z / len]
}

pub fn scalep(pt: Point, t: f64) -> Point {
    let [x, y, z] = pt;

    return [x * t, y * t, z * t];
}

pub fn addp(pt0: Point, pt1: Point) -> Point {
    let [x0, y0, z0] = pt0;
    let [x1, y1, z1] = pt1;

    return [x0 + x1, y0 + y1, z0 + z1];
}

pub fn subp(pt0: Point, pt1: Point) -> Point {
    let [x0, y0, z0] = pt0;
    let [x1, y1, z1] = pt1;

    return [x0 - x1, y0 - y1, z0 - z1];
}

pub fn dotp(pt0: Point, pt1: Point) -> f64 {
    let [x0, y0, z0] = pt0;
    let [x1, y1, z1] = pt1;

    return x0 * x1 + y0 * y1 + z0 * z1;
}

pub fn negp(pt: Point) -> Point {
    let [x, y, z] = pt;

    return [-x, -y, -z];
}
