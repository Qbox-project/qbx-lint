// Generates crates/qbx_fivem_data/stubs/glm.lua from the registration tables of the CfxLua GLM binding.
// usage: node scripts/generate-glm-stub.mjs [folder-or-base-url-of-the-binding-sources]
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SOURCE = 'https://raw.githubusercontent.com/citizenfx/lua/luaglm-dev/cfx/libs/glm-binding';
const base = (process.argv[2] ?? SOURCE).replace(/[\\/]$/, '');
const read = async (file) =>
    /^https?:/.test(base) ? (await fetch(`${base}/${file}`)).text() : readFileSync(join(base, file), 'utf8');

const ENTRY = /\{\s*"([A-Za-z_]\w*)"\s*,\s*(\w+)(?:\((\w+)\))?\s*\}|GLM_LUA_REG\((\w+)\)/g;
const isPublic = (name) => !name.startsWith('__') && !name.startsWith('operator_');

/** name -> true for constants, false for functions */
const names = new Map();
/** alias -> the function it is another name of, e.g. tointeger -> toint */
const aliases = new Map();
for (const match of (await read('lglmlib_reg.hpp')).matchAll(ENTRY)) {
    const name = match[1] ?? match[4];
    if (names.has(name)) {
        continue;
    }
    names.set(name, match[2] === 'GLM_NULLPTR');
    if (match[2] === 'GLM_NAME') {
        aliases.set(name, match[3]);
    }
}
if (names.size < 500) {
    throw new Error(`only ${names.size} names found in ${base}; the registration table moved or changed format`);
}

const library = await read('lglmlib.cpp');
const registers = (name) => library.includes(`"${name}"`);
const LIBRARY_NUMBERS = ['eps', 'feps', 'huge', 'maxinteger', 'mininteger', 'FP_INFINITE', 'FP_NAN', 'FP_ZERO', 'FP_SUBNORMAL', 'FP_NORMAL'];
LIBRARY_NUMBERS.filter(registers).forEach((name) => names.set(name, true));
['type', 'random', 'randomseed'].filter(registers).forEach((name) => names.set(name, false));

/** sub-library (glm.polygon, glm.aabb, ...) -> its function names */
const geometry = new Map();
const geom = await read('geom.hpp');
const registrations = [
    ...[...library.matchAll(/luaglm_(\w+?)lib\);\s*lua_setfield\(L, -2, "(\w+)"\);/g)].map((m) => [m[1], m[2]]),
    ...[...library.matchAll(/"(\w+)",\s*luaglm_(\w+?)lib\);/g)].map((m) => [m[2], m[1]]),
];
for (const [table, name] of registrations) {
    const body = new RegExp(`luaglm_${table}lib\\[\\]\\s*=\\s*\\{([\\s\\S]*?)\\n\\};`).exec(geom);
    if (body) {
        const members = [...body[1].matchAll(ENTRY)].map((m) => m[1] ?? m[4]).filter(isPublic);
        geometry.set(name, [...new Set(members)].sort((a, b) => a.localeCompare(b)));
    }
}
if (!geometry.has('polygon')) {
    throw new Error('the geometry libraries were not found; lglmlib.cpp or geom.hpp changed format');
}

const T = 'glm.num';
const V = 'glm.vec';
const unary = (doc) => ({ doc, params: [['x', T]], returns: T });
const binary = (doc, a = 'x', b = 'y') => ({ doc, params: [[a, T], [b, T]], returns: T });
const ctor = (type, doc) => ({ doc, params: [['...', 'number|glm.vec|quat|matrix']], returns: type });
const ease = { doc: 'Easing curve for `t` in the range 0..1.', params: [['t', 'number']], returns: 'number' };

const SIGNATURES = {
    vec: ctor(V, 'Builds a vector whose size follows from the arguments.'),
    vec1: ctor('number', 'Builds a one component vector, which CfxLua represents as a number.'),
    vec2: ctor('vector2', 'Builds a vector2 from numbers and/or other vectors.'),
    vec3: ctor('vector3', 'Builds a vector3 from numbers and/or other vectors.'),
    vec4: ctor('vector4', 'Builds a vector4 from numbers and/or other vectors.'),
    ivec2: ctor('vector2', 'Builds a vector2 with every component truncated to an integer.'),
    ivec3: ctor('vector3', 'Builds a vector3 with every component truncated to an integer.'),
    ivec4: ctor('vector4', 'Builds a vector4 with every component truncated to an integer.'),
    quat: ctor('quat', 'Builds a quaternion from `w, x, y, z`, an angle and axis, two directions, or a rotation matrix.'),
    qua: ctor('quat', 'Alias of `glm.quat`.'),
    mat: ctor('matrix', 'Builds a matrix whose size follows from the arguments.'),
    mat2: ctor('matrix', 'Builds a 2x2 matrix.'),
    mat3: ctor('matrix', 'Builds a 3x3 matrix from numbers, column vectors or a quaternion.'),
    mat4: ctor('matrix', 'Builds a 4x4 matrix from numbers, column vectors or a quaternion.'),

    abs: unary('Absolute value, per component.'),
    ceil: unary('Rounds up, per component.'),
    floor: unary('Rounds down, per component.'),
    round: unary('Rounds to the nearest integer, per component.'),
    trunc: unary('Drops the fractional part, per component.'),
    fract: unary('Fractional part `x - floor(x)`, per component.'),
    sign: unary('-1, 0 or 1 depending on the sign, per component.'),
    sqrt: unary('Square root, per component.'),
    inversesqrt: unary('`1 / sqrt(x)`, per component.'),
    exp: unary('Natural exponentiation, per component.'),
    exp2: unary('`2 ^ x`, per component.'),
    log: unary('Natural logarithm, per component.'),
    log2: unary('Base 2 logarithm, per component.'),
    sin: unary('Sine of an angle in radians, per component.'),
    cos: unary('Cosine of an angle in radians, per component.'),
    tan: unary('Tangent of an angle in radians, per component.'),
    asin: unary('Arc sine in radians, per component.'),
    acos: unary('Arc cosine in radians, per component.'),
    sinh: unary('Hyperbolic sine, per component.'),
    cosh: unary('Hyperbolic cosine, per component.'),
    tanh: unary('Hyperbolic tangent, per component.'),
    degrees: unary('Converts radians to degrees, per component.'),
    radians: unary('Converts degrees to radians, per component.'),
    saturate: unary('Clamps to the range 0..1, per component.'),
    atan: { doc: 'Arc tangent in radians; with two arguments the quadrant follows from the signs of `y` and `x`.', params: [['y', T], ['x?', T]], returns: T },
    atan2: binary('Arc tangent of `y / x` in radians, using the signs to pick the quadrant.', 'y', 'x'),
    mod: binary('Modulus `x - y * floor(x / y)`, per component.'),
    fmod: binary('Remainder of `x / y` with the sign of `x`, per component.'),
    pow: binary('`base ^ exponent`, per component.', 'base', 'exponent'),
    min: { doc: 'Smallest of the arguments, per component.', params: [['x', T], ['y', T], ['...', T]], returns: T },
    max: { doc: 'Largest of the arguments, per component.', params: [['x', T], ['y', T], ['...', T]], returns: T },
    step: binary('0 where `x < edge`, otherwise 1, per component.', 'edge', 'x'),
    clamp: { doc: 'Restricts `x` to `minVal..maxVal`; with one argument to 0..1.', params: [['x', T], ['minVal?', T], ['maxVal?', T]], returns: T },
    mix: { doc: 'Linear blend `x * (1 - a) + y * a`. Quaternions are blended spherically.', params: [['x', `${T}|quat`], ['y', `${T}|quat`], ['a', T]], returns: `${T}|quat` },
    lerp: { doc: 'Linear interpolation between `x` and `y`.', params: [['x', `${T}|quat`], ['y', `${T}|quat`], ['a', T]], returns: `${T}|quat` },
    slerp: { doc: 'Spherical interpolation between two quaternions or directions.', params: [['x', `quat|${V}`], ['y', `quat|${V}`], ['a', 'number']], returns: `quat|${V}` },
    smoothstep: { doc: 'Hermite interpolation between 0 and 1 while `x` moves from `edge0` to `edge1`.', params: [['edge0', T], ['edge1', T], ['x', T]], returns: T },

    dot: { doc: 'Dot product.', params: [['x', `${V}|quat`], ['y', `${V}|quat`]], returns: 'number' },
    cross: { doc: 'Cross product.', params: [['x', 'vector3'], ['y', 'vector3']], returns: 'vector3' },
    length: { doc: 'Length of a vector or quaternion.', params: [['x', `${T}|quat`]], returns: 'number' },
    length2: { doc: 'Squared length, which avoids the square root.', params: [['x', `${T}|quat`]], returns: 'number' },
    distance: { doc: 'Distance between two points.', params: [['p0', T], ['p1', T]], returns: 'number' },
    distance2: { doc: 'Squared distance between two points.', params: [['p0', T], ['p1', T]], returns: 'number' },
    normalize: { doc: 'Same direction, length 1.', params: [['x', `${V}|quat`]], returns: `${V}|quat` },
    reflect: { doc: 'Reflects the incident vector `I` around the normal `N`.', params: [['I', V], ['N', V]], returns: V },
    refract: { doc: 'Refracts `I` through a surface with normal `N` and ratio of indices `eta`.', params: [['I', V], ['N', V], ['eta', 'number']], returns: V },
    faceforward: { doc: 'Returns `N` flipped so that it faces against `I`.', params: [['N', V], ['I', V], ['Nref', V]], returns: V },
    angle: { doc: 'Angle in radians between two normalized vectors, or the rotation angle of a quaternion.', params: [['x', `${V}|quat`], ['y?', V]], returns: 'number' },

    toint: { doc: 'Converts to an integer when the value has an exact integer representation, per component.', params: [['x', T]], returns: 'integer|glm.vec' },
    all: { doc: 'True when every component is truthy.', params: [['v', 'any']], returns: 'boolean' },
    any: { doc: 'True when at least one component is truthy.', params: [['v', 'any']], returns: 'boolean' },
    isnan: { doc: 'Whether the value is NaN, per component.', params: [['x', T]], returns: 'boolean|glm.vec' },
    isinf: { doc: 'Whether the value is infinite, per component.', params: [['x', T]], returns: 'boolean|glm.vec' },
    equal: { doc: 'Component-wise equality, optionally within `epsilon`.', params: [['x', 'any'], ['y', 'any'], ['epsilon?', 'number']], returns: 'boolean|glm.vec' },
    notEqual: { doc: 'Component-wise inequality, optionally outside `epsilon`.', params: [['x', 'any'], ['y', 'any'], ['epsilon?', 'number']], returns: 'boolean|glm.vec' },
    epsilonEqual: { doc: 'Whether `|x - y| < epsilon`, per component.', params: [['x', T], ['y', T], ['epsilon', T]], returns: 'boolean|glm.vec' },
    unpack: { doc: 'Returns the components of vectors, quaternions and matrices as separate values.', params: [['...', 'any']], returns: 'number ...' },
    to_string: { doc: 'Human readable text for a vector, quaternion or matrix.', params: [['x', 'any']], returns: 'string' },
    up: { doc: 'The unit vector pointing up.', params: [], returns: 'vector3' },
    right: { doc: 'The unit vector pointing right.', params: [], returns: 'vector3' },
    forward: { doc: 'The unit vector pointing forward.', params: [], returns: 'vector3' },

    conjugate: { doc: 'Conjugate of a quaternion.', params: [['q', 'quat']], returns: 'quat' },
    inverse: { doc: 'Inverse of a quaternion or matrix.', params: [['x', 'quat|matrix']], returns: 'quat|matrix' },
    axis: { doc: 'Rotation axis of a quaternion.', params: [['q', 'quat']], returns: 'vector3' },
    angleAxis: { doc: 'Quaternion that rotates by `angle` radians around `axis`.', params: [['angle', 'number'], ['axis', 'vector3']], returns: 'quat' },
    eulerAngles: { doc: 'Pitch, yaw and roll of a quaternion, in radians.', params: [['q', 'quat']], returns: 'vector3' },
    pitch: { doc: 'Pitch of a quaternion in radians.', params: [['q', 'quat']], returns: 'number' },
    yaw: { doc: 'Yaw of a quaternion in radians.', params: [['q', 'quat']], returns: 'number' },
    roll: { doc: 'Roll of a quaternion in radians.', params: [['q', 'quat']], returns: 'number' },
    quatLookAt: { doc: 'Quaternion that looks along `direction` with the given `up` vector.', params: [['direction', 'vector3'], ['up', 'vector3']], returns: 'quat' },
    rotate: { doc: 'Rotates a vector by a quaternion, or a quaternion/matrix by `angle` radians around `axis`.', params: [['x', `quat|matrix|${V}`], ['angle', `number|${V}`], ['axis?', 'vector3']], returns: `quat|matrix|${V}` },
    mat3_cast: { doc: 'Rotation matrix of a quaternion.', params: [['q', 'quat']], returns: 'matrix' },
    mat4_cast: { doc: 'Rotation matrix of a quaternion.', params: [['q', 'quat']], returns: 'matrix' },
    toMat3: { doc: 'Rotation matrix of a quaternion.', params: [['q', 'quat']], returns: 'matrix' },
    toMat4: { doc: 'Rotation matrix of a quaternion.', params: [['q', 'quat']], returns: 'matrix' },
    quat_cast: { doc: 'Quaternion of a rotation matrix.', params: [['m', 'matrix']], returns: 'quat' },
    toQuat: { doc: 'Quaternion of a rotation matrix.', params: [['m', 'matrix']], returns: 'quat' },

    transpose: { doc: 'Transposed matrix.', params: [['m', 'matrix']], returns: 'matrix' },
    determinant: { doc: 'Determinant of a square matrix.', params: [['m', 'matrix']], returns: 'number' },
    identity: { doc: 'Identity matrix.', params: [], returns: 'matrix' },
    translate: { doc: 'Appends a translation to a matrix.', params: [['m', 'matrix'], ['v', 'vector3']], returns: 'matrix' },
    scale: { doc: 'Appends a scale to a matrix.', params: [['m', 'matrix'], ['v', 'vector3']], returns: 'matrix' },
    lookAt: { doc: 'View matrix for a camera at `eye` looking at `center`.', params: [['eye', 'vector3'], ['center', 'vector3'], ['up', 'vector3']], returns: 'matrix' },
    perspective: { doc: 'Perspective projection matrix; `fovy` is in radians.', params: [['fovy', 'number'], ['aspect', 'number'], ['near', 'number'], ['far', 'number']], returns: 'matrix' },
    ortho: { doc: 'Orthographic projection matrix.', params: [['left', 'number'], ['right', 'number'], ['bottom', 'number'], ['top', 'number'], ['near?', 'number'], ['far?', 'number']], returns: 'matrix' },

    linearRand: { doc: 'Uniformly distributed random value between `min` and `max`, per component.', params: [['min', T], ['max', T]], returns: T },
    sphericalRand: { doc: 'Random point on a sphere of the given radius.', params: [['radius', 'number']], returns: 'vector3' },
    circularRand: { doc: 'Random point on a circle of the given radius.', params: [['radius', 'number']], returns: 'vector2' },
    ballRand: { doc: 'Random point inside a sphere of the given radius.', params: [['radius', 'number']], returns: 'vector3' },
    diskRand: { doc: 'Random point inside a circle of the given radius.', params: [['radius', 'number']], returns: 'vector2' },
};
for (const name of names.keys()) {
    if (/Ease(In|Out|InOut)$/.test(name) && !SIGNATURES[name]) {
        SIGNATURES[name] = ease;
    }
}

for (const [alias, target] of aliases) {
    if (SIGNATURES[target] && !SIGNATURES[alias]) {
        SIGNATURES[alias] = { ...SIGNATURES[target], doc: `Alias of \`glm.${target}\`. ${SIGNATURES[target].doc}` };
    }
}

const POLYGON = 'glm.polygon';
const GEOMETRY_SIGNATURES = {
    'polygon.new': { doc: 'Builds a polygon from its corner points.', params: [['points', 'vector3[]']], returns: POLYGON },
    'polygon.contains': { doc: 'Whether `point` lies within the polygon, allowing `thickness` along its normal.', params: [['self', POLYGON], ['point', 'vector3'], ['thickness?', 'number']], returns: 'boolean' },
    'polygon.area': { doc: 'Surface area of the polygon.', params: [['self', POLYGON]], returns: 'number' },
    'polygon.perimeter': { doc: 'Summed length of the edges.', params: [['self', POLYGON]], returns: 'number' },
    'polygon.centroid': { doc: 'Centre of mass of the polygon.', params: [['self', POLYGON]], returns: 'vector3' },
    'polygon.isConvex': { doc: 'Whether the polygon is convex.', params: [['self', POLYGON]], returns: 'boolean' },
    'polygon.isPlanar': { doc: 'Whether all points lie in one plane.', params: [['self', POLYGON]], returns: 'boolean' },
};

const missing = [
    ...Object.keys(SIGNATURES).filter((name) => !names.has(name)),
    ...Object.keys(GEOMETRY_SIGNATURES).filter((key) => {
        const [library, member] = key.split('.');
        return !geometry.get(library)?.includes(member);
    }),
];
if (missing.length > 0) {
    throw new Error(`described but not registered by the binding: ${missing.join(', ')}`);
}

const lines = [
    '---@meta',
    '-- Generated by scripts/generate-glm-stub.mjs from the registration tables of the CfxLua GLM binding',
    `-- (${SOURCE}).`,
    '-- Functions without a description are registered by the runtime but not described here; they',
    '-- follow the GLM function of the same name.',
    '',
    '---A matrix of 2 to 4 columns and rows.',
    '---@class matrix',
    '',
    '---@alias glm.vec vector2|vector3|vector4',
    '---@alias glm.num number|vector2|vector3|vector4',
    '',
    '---GLM mathematics for vectors, quaternions and matrices, built into CfxLua.',
    '---@class glm',
    'glm = {}',
    '',
];
const sorted = [...names.keys()].sort((a, b) => a.localeCompare(b));
for (const name of sorted.filter((n) => names.get(n))) {
    lines.push('---@type number', `glm.${name} = 0`, '');
}
const emit = (path, signature) => {
    if (!signature) {
        lines.push('---@param ... any', '---@return any', `function ${path}(...) end`, '');
        return;
    }
    lines.push(`---${signature.doc}`);
    for (const [param, type] of signature.params) {
        lines.push(`---@param ${param} ${type}`);
    }
    lines.push(`---@return ${signature.returns}`);
    const args = signature.params.map(([param]) => param.replace('?', '')).join(', ');
    lines.push(`function ${path}(${args}) end`, '');
};
for (const name of sorted.filter((n) => !names.get(n))) {
    emit(`glm.${name}`, SIGNATURES[name]);
}
for (const [library, members] of [...geometry].sort(([a], [b]) => a.localeCompare(b))) {
    lines.push(`---Geometry helpers for ${library} shapes.`, `---@class glm.${library}`, `glm.${library} = {}`, '');
    for (const member of members) {
        emit(`glm.${library}.${member}`, GEOMETRY_SIGNATURES[`${library}.${member}`]);
    }
}

const target = join(dirname(fileURLToPath(import.meta.url)), '..', 'crates', 'qbx_fivem_data', 'stubs', 'glm.lua');
writeFileSync(target, lines.join('\n'));
const geometryCount = [...geometry.values()].reduce((sum, members) => sum + members.length, 0);
console.log(`wrote ${sorted.length} glm members and ${geometryCount} in ${geometry.size} geometry libraries to ${target}`);
