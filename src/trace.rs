use std::collections::HashMap;

macro_rules! settings {
    ($($field:ident = $key:literal),* $(,)?) => {
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct Settings { $(pub $field: f32,)* }

        impl Settings {
            fn from_map(mut m: HashMap<String, f32>) -> Result<Self, String> {
                let s = Settings {
                    $($field: m.remove($key).ok_or(concat!("settings: missing ", $key))?,)*
                };
                match m.keys().next() {
                    Some(k) => Err(format!("settings: unknown key {k}")),
                    None => Ok(s),
                }
            }
        }
    };
}

settings! {
    gravity = "gravity",
    speed_scale = "speedScale",
    accel_scale = "accelScale",
    time_scale = "timeScale",
    friction_scale = "frictionScale",
    lagged = "lagged",
    air_speed = "airSpeed",
    air_acceleration = "airAcceleration",
    normal_x = "normalX",
    normal_y = "normalY",
    normal_z = "normalZ",
    friction_ramp_end = "frictionRampEnd",
    weapon_speed_scale = "weaponSpeedScale",
    movetype = "movetype",
    stance = "stance",
    low_speed = "lowSpeed",
    speed = "speed",
    sprint_speed = "sprintspeed",
    acceleration = "acceleration",
    deceleration = "deceleration",
    low_acceleration = "lowAcceleration",
    sprint_acceleration = "sprintAcceleration",
    sprint_deceleration = "sprintDeceleration",
    speed_scale_side = "speedScaleSide",
    speed_scale_back = "speedScaleBack",
    gravity_scale = "gravityScale",
    air_drag = "airDrag",
    air_drag_constant = "airDragConstant",
    air_strafe_enabled = "airStrafeEnabled",
    sv_gravity = "sv_gravity",
    extra_air_accel = "player_extraairaccelleration",
}

pub const FLAG_SPRINT: u32 = 1;
pub const FLAG_DUCKED: u32 = 2;
pub const FLAG_SLIDE: u32 = 4;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cmd {
    pub seq: u32,
    pub frametime: f32,
    pub curtime: f32,
    pub angles: [f32; 3],
    pub forward: f32,
    pub side: f32,
    pub up: f32,
    pub buttons: u32,
    pub oldbuttons: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub flags: u32,
    pub onground: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub cmd: Cmd,
    pub input: State,
    pub output: State,
    pub maxspeed: f32,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub struct Trace {
    pub map: String,
    pub settings: Settings,
    pub steps: Vec<Step>,
}

struct Fields<'a> {
    it: std::str::SplitWhitespace<'a>,
    line: usize,
}

impl Fields<'_> {
    fn next(&mut self) -> Result<&str, String> {
        self.it.next().ok_or(format!("line {}: too few fields", self.line))
    }

    fn int(&mut self) -> Result<u32, String> {
        let line = self.line;
        let t = self.next()?;
        u32::from_str_radix(t, 16).map_err(|_| format!("line {line}: bad hex {t:?}"))
    }

    fn float(&mut self) -> Result<f32, String> {
        let line = self.line;
        let t = self.next()?;
        let f = match (t.len(), u32::from_str_radix(t, 16)) {
            (8, Ok(b)) => f32::from_bits(b),
            _ => return Err(format!("line {line}: bad f32 hex {t:?}")),
        };
        if f.is_nan() {
            return Err(format!("line {line}: NaN"));
        }
        Ok(f)
    }

    fn vec3(&mut self) -> Result<[f32; 3], String> {
        Ok([self.float()?, self.float()?, self.float()?])
    }

    fn state(&mut self) -> Result<State, String> {
        Ok(State {
            origin: self.vec3()?,
            velocity: self.vec3()?,
            flags: self.int()?,
            onground: self.int()? != 0,
        })
    }

    fn end(mut self) -> Result<(), String> {
        match self.it.next() {
            Some(t) => Err(format!("line {}: unexpected field {t:?}", self.line)),
            None => Ok(()),
        }
    }
}

fn expect<'a>(lines: &mut impl Iterator<Item = (usize, &'a str)>, tag: &str, after: usize) -> Result<Fields<'a>, String> {
    let (n, l) = lines.next().ok_or(format!("line {}: {tag} line missing at end of file", after + 1))?;
    let mut it = l.split_whitespace();
    if it.next() != Some(tag) {
        return Err(format!("line {n}: expected {tag}"));
    }
    Ok(Fields { it, line: n })
}

pub fn parse(text: &str) -> Result<Trace, String> {
    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l)).filter(|(_, l)| !l.trim().is_empty());

    let (n, header) = lines.next().ok_or("line 1: empty trace")?;
    let mut h = header.split_whitespace();
    if (h.next(), h.next()) != (Some("#"), Some("lurch-trace")) {
        return Err(format!("line {n}: not a lurch trace"));
    }
    if h.next() != Some("1") {
        return Err(format!("line {n}: unknown trace version"));
    }
    let map = h.find_map(|kv| kv.strip_prefix("map=")).unwrap_or("unknown").to_string();

    let mut f = expect(&mut lines, "settings", n)?;
    let mut m = HashMap::new();
    while let Some(kv) = f.it.next() {
        let (k, v) = kv.split_once('=').ok_or(format!("line {}: bad setting {kv:?}", f.line))?;
        let mut one = Fields { it: v.split_whitespace(), line: f.line };
        m.insert(k.to_string(), one.float()?);
    }
    let settings = Settings::from_map(m).map_err(|e| format!("line {}: {e}", f.line))?;

    let mut steps = Vec::new();
    while let Some((n, l)) = lines.next() {
        let mut c = l.split_whitespace();
        if c.next() != Some("cmd") {
            return Err(format!("line {n}: expected cmd"));
        }
        let mut c = Fields { it: c, line: n };
        let cmd = Cmd {
            seq: c.int()?,
            frametime: c.float()?,
            curtime: c.float()?,
            angles: c.vec3()?,
            forward: c.float()?,
            side: c.float()?,
            up: c.float()?,
            buttons: c.int()?,
            oldbuttons: c.int()?,
        };
        c.end()?;
        let mut i = expect(&mut lines, "in", n)?;
        let input = i.state()?;
        let in_line = i.line;
        i.end()?;
        let mut o = expect(&mut lines, "out", in_line)?;
        let output = o.state()?;
        let maxspeed = o.float()?;
        o.end()?;
        steps.push(Step { cmd, input, output, maxspeed, line: n });
    }
    Ok(Trace { map, settings, steps })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYS: &str = "gravity speedScale accelScale timeScale frictionScale lagged airSpeed airAcceleration \
        normalX normalY normalZ frictionRampEnd weaponSpeedScale movetype stance lowSpeed speed sprintspeed \
        acceleration deceleration lowAcceleration sprintAcceleration sprintDeceleration speedScaleSide \
        speedScaleBack gravityScale airDrag airDragConstant airStrafeEnabled sv_gravity player_extraairaccelleration";

    fn trace(body: &str) -> String {
        let settings: Vec<String> = KEYS.split_whitespace().map(|k| format!("{k}=3f800000")).collect();
        format!("# lurch-trace 1 build=5dba2a2f-02ae8471 map=test\nsettings {}\n{body}", settings.join(" "))
    }

    const CMD: &str = "cmd 10 3c888889 42c80000 00000000 00000000 00000000 3f800000 00000000 00000000 8 0";
    const IN: &str = "in  00000000 00000000 00000000 00000000 00000000 00000000 0 1";
    const OUT: &str = "out 00000000 00000000 00000000 00000000 00000000 00000000 0 1 43340000";

    #[test]
    fn parses_a_triple() {
        let t = parse(&trace(&format!("{CMD}\n{IN}\n{OUT}\n"))).unwrap();
        assert_eq!(t.map, "test");
        assert_eq!(t.steps.len(), 1);
        assert_eq!(t.steps[0].cmd.seq, 0x10);
        assert_eq!(t.steps[0].cmd.forward, 1.0);
        assert_eq!(t.steps[0].maxspeed, 180.0);
        assert!(t.steps[0].input.onground);
    }

    #[test]
    fn rejects_nan_naming_the_line() {
        let e = parse(&trace(&format!("{CMD}\n{}\n{OUT}\n", IN.replacen("00000000", "7fc00000", 1)))).unwrap_err();
        assert_eq!(e, "line 4: NaN");
    }

    #[test]
    fn rejects_orphan_lines() {
        assert_eq!(parse(&trace(&format!("{CMD}\n{OUT}\n"))).unwrap_err(), "line 4: expected in");
        assert_eq!(parse(&trace(&format!("{IN}\n"))).unwrap_err(), "line 3: expected cmd");
        assert_eq!(parse(&trace(&format!("{CMD}\n{IN}\n"))).unwrap_err(), "line 5: out line missing at end of file");
    }

    #[test]
    fn rejects_unknown_version() {
        let t = trace("").replace("lurch-trace 1", "lurch-trace 2");
        assert_eq!(parse(&t).unwrap_err(), "line 1: unknown trace version");
    }

    #[test]
    fn rejects_bad_hex() {
        let e = parse(&trace(&format!("{}\n{IN}\n{OUT}\n", CMD.replace("3c888889", "3c88z889")))).unwrap_err();
        assert_eq!(e, "line 3: bad f32 hex \"3c88z889\"");
    }
}
