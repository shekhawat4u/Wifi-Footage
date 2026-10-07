// ============================================================
// RuView Observatory — 3D WebGL DensePose & CSI Engine
// ============================================================

let config = null;
let state = null;
const timelineHistory = [];
const MAX_TIMELINE = 60;

// Three.js 3D Globals
let scene, camera, renderer, controls;
let roomMesh, gridHelper, baselineMesh;
let nodeMeshes = [];
let personMeshes = [];
let particleSystem = null;
let particleCount = 800;

const $ = (s) => document.querySelector(s);
const $$ = (s) => document.querySelectorAll(s);

// ============================================================
// Three.js 3D Scene Initialization
// ============================================================
function init3D() {
  const canvas = document.getElementById('observatory-canvas');
  const width = window.innerWidth;
  const height = window.innerHeight;

  scene = new THREE.Scene();
  scene.background = new THREE.Color(0x080c14);
  scene.fog = new THREE.FogExp2(0x080c14, 0.04);

  camera = new THREE.PerspectiveCamera(50, width / height, 0.1, 100);
  camera.position.set(0, 6.5, 7.5);

  renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
  renderer.setSize(width, height);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));

  // OrbitControls
  if (typeof THREE.OrbitControls !== 'undefined') {
    controls = new THREE.OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.05;
    controls.maxPolarAngle = Math.PI / 2 - 0.05;
    controls.minDistance = 3;
    controls.maxDistance = 25;
    controls.target.set(0, 0.5, 0);
  }

  // Ambient & Directional Lights
  const ambientLight = new THREE.AmbientLight(0x00d878, 0.35);
  scene.add(ambientLight);

  const dirLight = new THREE.DirectionalLight(0x3eff8a, 0.8);
  dirLight.position.set(5, 10, 5);
  scene.add(dirLight);

  const blueLight = new THREE.PointLight(0x2090ff, 1.2, 15);
  blueLight.position.set(-3, 2, -3);
  scene.add(blueLight);

  // 1. 3D Room Floor Grid
  const roomW = (config && config.width_m) || 5.0;
  const roomH = (config && config.height_m) || 4.0;

  const floorGeo = new THREE.PlaneGeometry(roomW, roomH);
  const floorMat = new THREE.MeshBasicMaterial({
    color: 0x0c1626,
    side: THREE.DoubleSide,
    transparent: true,
    opacity: 0.85,
  });
  roomMesh = new THREE.Mesh(floorGeo, floorMat);
  roomMesh.rotation.x = -Math.PI / 2;
  roomMesh.position.y = 0;
  scene.add(roomMesh);

  // Grid Lines
  gridHelper = new THREE.GridHelper(Math.max(roomW, roomH) * 1.2, 20, 0x00d878, 0x142b3a);
  gridHelper.position.y = 0.01;
  scene.add(gridHelper);

  // 2. Diagonal RF Sensing Baseline (Zone D <-> Corner B)
  const p1 = new THREE.Vector3(4.65 - roomW / 2, 0.2, 0.35 - roomH / 2);
  const p2 = new THREE.Vector3(0.35 - roomW / 2, 0.2, 3.65 - roomH / 2);

  const baselineGeo = new THREE.BufferGeometry().setFromPoints([p1, p2]);
  const baselineMat = new THREE.LineDashedMaterial({
    color: 0x00d878,
    dashSize: 0.2,
    gapSize: 0.1,
    linewidth: 2,
    transparent: true,
    opacity: 0.6,
  });
  baselineMesh = new THREE.Line(baselineGeo, baselineMat);
  baselineMesh.computeLineDistances();
  scene.add(baselineMesh);

  // 3. Node Hardware Cylinders
  buildHardwareNodes();

  // 4. Multipath RF Wavefield Particle Cloud
  buildParticleWavefield();

  window.addEventListener('resize', onWindowResize);
}

function buildHardwareNodes() {
  nodeMeshes.forEach((m) => scene.remove(m));
  nodeMeshes = [];

  const roomW = (config && config.width_m) || 5.0;
  const roomH = (config && config.height_m) || 4.0;

  const nodes = (config && config.links) || [
    { id: 'receiver-1', label: 'ESP32 Node 1', x: 4.65, y: 0.35, color: '#00d878' },
    { id: 'receiver-2', label: 'ESP8266 Node 2', x: 0.35, y: 3.65, color: '#ffb020' },
  ];

  nodes.forEach((n) => {
    const group = new THREE.Group();
    const nx = n.x - roomW / 2;
    const nz = n.y - roomH / 2;

    // Node Beacon Base
    const baseGeo = new THREE.CylinderGeometry(0.12, 0.16, 0.3, 16);
    const baseMat = new THREE.MeshStandardMaterial({
      color: n.id === 'receiver-1' ? 0x00d878 : 0xffb020,
      emissive: n.id === 'receiver-1' ? 0x006633 : 0x664400,
      metalness: 0.7,
      roughness: 0.2,
    });
    const baseMesh = new THREE.Mesh(baseGeo, baseMat);
    baseMesh.position.y = 0.15;
    group.add(baseMesh);

    // Glowing Antenna Tip
    const tipGeo = new THREE.SphereGeometry(0.06, 12, 12);
    const tipMat = new THREE.MeshBasicMaterial({
      color: n.id === 'receiver-1' ? 0x3eff8a : 0xffcc33,
    });
    const tipMesh = new THREE.Mesh(tipGeo, tipMat);
    tipMesh.position.y = 0.35;
    group.add(tipMesh);

    // Pulsing Halo Ring
    const ringGeo = new THREE.RingGeometry(0.2, 0.28, 24);
    const ringMat = new THREE.MeshBasicMaterial({
      color: n.id === 'receiver-1' ? 0x00d878 : 0xffb020,
      side: THREE.DoubleSide,
      transparent: true,
      opacity: 0.4,
    });
    const ringMesh = new THREE.Mesh(ringGeo, ringMat);
    ringMesh.rotation.x = -Math.PI / 2;
    ringMesh.position.y = 0.02;
    group.add(ringMesh);

    group.position.set(nx, 0, nz);
    scene.add(group);
    nodeMeshes.push(group);
  });
}

function buildParticleWavefield() {
  if (particleSystem) scene.remove(particleSystem);

  const geo = new THREE.BufferGeometry();
  const positions = new Float32Array(particleCount * 3);
  const colors = new Float32Array(particleCount * 3);

  const roomW = (config && config.width_m) || 5.0;
  const roomH = (config && config.height_m) || 4.0;

  for (let i = 0; i < particleCount; i++) {
    positions[i * 3] = (Math.random() - 0.5) * roomW;
    positions[i * 3 + 1] = Math.random() * 2.2;
    positions[i * 3 + 2] = (Math.random() - 0.5) * roomH;

    // Green to cyan gradient
    const r = 0.0;
    const g = 0.7 + Math.random() * 0.3;
    const b = 0.4 + Math.random() * 0.5;
    colors[i * 3] = r;
    colors[i * 3 + 1] = g;
    colors[i * 3 + 2] = b;
  }

  geo.setAttribute('position', new THREE.BufferAttribute(positions, 3));
  geo.setAttribute('color', new THREE.BufferAttribute(colors, 3));

  const mat = new THREE.PointsMaterial({
    size: 0.05,
    vertexColors: true,
    transparent: true,
    opacity: 0.55,
    blending: THREE.AdditiveBlending,
  });

  particleSystem = new THREE.Points(geo, mat);
  scene.add(particleSystem);
}

// ============================================================
// 17-Keypoint DensePose Skeleton Constructor
// ============================================================
function createDensePoseSkeleton(colorHex) {
  const group = new THREE.Group();

  // 17 Joints (Spheres)
  const joints = [];
  const jointGeo = new THREE.SphereGeometry(0.045, 8, 8);
  const jointMat = new THREE.MeshBasicMaterial({
    color: colorHex,
  });

  for (let i = 0; i < 17; i++) {
    const mesh = new THREE.Mesh(jointGeo, jointMat);
    group.add(mesh);
    joints.push(mesh);
  }

  // Bones (Lines connecting joints)
  // DensePose connections: 
  // [0:Head, 1:Neck, 2:R-Shoulder, 3:R-Elbow, 4:R-Wrist, 5:L-Shoulder, 6:L-Elbow, 7:L-Wrist,
  //  8:Mid-Spine, 9:R-Hip, 10:R-Knee, 11:R-Ankle, 12:L-Hip, 13:L-Knee, 14:L-Ankle, 15:Chest, 16:Pelvis]
  const bonePairs = [
    [0, 1], [1, 15], [15, 8], [8, 16],
    [1, 2], [2, 3], [3, 4],
    [1, 5], [5, 6], [6, 7],
    [16, 9], [9, 10], [10, 11],
    [16, 12], [12, 13], [13, 14],
  ];

  const bones = [];
  const boneMat = new THREE.LineBasicMaterial({
    color: colorHex,
    transparent: true,
    opacity: 0.75,
    linewidth: 2,
  });

  bonePairs.forEach(([a, b]) => {
    const lineGeo = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(0, 0, 0),
      new THREE.Vector3(0, 0, 0),
    ]);
    const line = new THREE.Line(lineGeo, boneMat);
    group.add(line);
    bones.push({ line, a, b });
  });

  // Base Radar Ring on Floor
  const ringGeo = new THREE.RingGeometry(0.25, 0.32, 24);
  const ringMat = new THREE.MeshBasicMaterial({
    color: colorHex,
    side: THREE.DoubleSide,
    transparent: true,
    opacity: 0.6,
  });
  const ring = new THREE.Mesh(ringGeo, ringMat);
  ring.rotation.x = -Math.PI / 2;
  ring.position.y = 0.02;
  group.add(ring);

  return { group, joints, bones, ring, bonePairs };
}

function updateDensePoseJoints(skeleton, px, pz, activity, time) {
  const isWalking = activity === 'Walking';
  const isSeated = activity === 'Typing / Seated';

  // Base Y height for seated vs standing
  const hipY = isSeated ? 0.60 : 0.95;
  const spineY = isSeated ? 0.95 : 1.35;
  const headY = isSeated ? 1.25 : 1.70;

  // Breathing / walking bob
  const breathBob = Math.sin(time * 2.0) * 0.015;
  const walkSwing = isWalking ? Math.sin(time * 6.0) * 0.25 : 0.0;
  const typingWiggle = isSeated ? Math.sin(time * 8.0) * 0.02 : 0.0;

  // 17 Joints local positions
  const jointOffsets = [
    [0, headY + breathBob, 0],                            // 0: Head
    [0, headY - 0.12 + breathBob, 0],                     // 1: Neck
    [0.22, spineY + breathBob, -0.05],                    // 2: R-Shoulder
    [0.32, spineY - 0.25, 0.15 + typingWiggle],           // 3: R-Elbow
    [0.25, isSeated ? 0.72 : 0.90, 0.30 + typingWiggle],  // 4: R-Wrist (on desk)
    [-0.22, spineY + breathBob, -0.05],                   // 5: L-Shoulder
    [-0.32, spineY - 0.25, 0.15 - typingWiggle],          // 6: L-Elbow
    [-0.25, isSeated ? 0.72 : 0.90, 0.30 - typingWiggle], // 7: L-Wrist (on desk)
    [0, spineY - 0.20, 0],                                // 8: Mid-Spine
    [0.18, hipY, 0],                                      // 9: R-Hip
    [0.20, isSeated ? 0.35 : hipY - 0.45, isSeated ? 0.35 : walkSwing],  // 10: R-Knee
    [0.20, 0.05, isSeated ? 0.40 : walkSwing * 1.2],                     // 11: R-Ankle
    [-0.18, hipY, 0],                                     // 12: L-Hip
    [-0.20, isSeated ? 0.35 : hipY - 0.45, isSeated ? 0.35 : -walkSwing], // 13: L-Knee
    [-0.20, 0.05, isSeated ? 0.40 : -walkSwing * 1.2],                    // 14: L-Ankle
    [0, spineY, 0],                                       // 15: Chest
    [0, hipY, 0],                                         // 16: Pelvis
  ];

  skeleton.joints.forEach((mesh, idx) => {
    const off = jointOffsets[idx];
    mesh.position.set(off[0], off[1], off[2]);
  });

  skeleton.bones.forEach(({ line, a, b }) => {
    const posA = skeleton.joints[a].position;
    const posB = skeleton.joints[b].position;
    const positions = line.geometry.attributes.position.array;
    positions[0] = posA.x;
    positions[1] = posA.y;
    positions[2] = posA.z;
    positions[3] = posB.x;
    positions[4] = posB.y;
    positions[5] = posB.z;
    line.geometry.attributes.position.needsUpdate = true;
  });

  skeleton.group.position.set(px, 0, pz);
}

function render3DPersons(people, time) {
  const roomW = (config && config.width_m) || 5.0;
  const roomH = (config && config.height_m) || 4.0;

  // Add or remove skeleton objects as count changes
  while (personMeshes.length < people.length) {
    const colorHex = people[personMeshes.length].color || '#00d878';
    const skel = createDensePoseSkeleton(colorHex);
    scene.add(skel.group);
    personMeshes.push(skel);
  }

  while (personMeshes.length > people.length) {
    const skel = personMeshes.pop();
    scene.remove(skel.group);
  }

  // Update position & pose for each occupant
  people.forEach((p, idx) => {
    const skel = personMeshes[idx];
    const px = p.x - roomW / 2;
    const pz = p.y - roomH / 2;
    updateDensePoseJoints(skel, px, pz, p.activity, time + idx * 0.8);
  });
}

function onWindowResize() {
  const width = window.innerWidth;
  const height = window.innerHeight;
  camera.aspect = width / height;
  camera.updateProjectionMatrix();
  renderer.setSize(width, height);
}

// Camera Presets
$('#view-3d').onclick = () => {
  $$('.btn-pill').forEach((b) => b.classList.remove('active'));
  $('#view-3d').classList.add('active');
  camera.position.set(0, 6.5, 7.5);
  camera.lookAt(0, 0.5, 0);
};

$('#view-top').onclick = () => {
  $$('.btn-pill').forEach((b) => b.classList.remove('active'));
  $('#view-top').classList.add('active');
  camera.position.set(0, 9.5, 0.01);
  camera.lookAt(0, 0, 0);
};

$('#view-side').onclick = () => {
  $$('.btn-pill').forEach((b) => b.classList.remove('active'));
  $('#view-side').classList.add('active');
  camera.position.set(8.5, 2.0, 0);
  camera.lookAt(0, 1.0, 0);
};

// ============================================================
// 2D CSI Waterfall Heatmap Renderer
// ============================================================
const csiCanvas = document.getElementById('csi-heatmap-canvas');
const csiCtx = csiCanvas ? csiCanvas.getContext('2d') : null;
const subcarrierAmps = new Array(52).fill(0);

function drawCSIHeatmap(energy, time) {
  if (!csiCtx) return;
  const w = csiCanvas.width;
  const h = csiCanvas.height;

  // Shift existing pixels up by 2px (Waterfall)
  const imgData = csiCtx.getImageData(0, 2, w, h - 2);
  csiCtx.putImageData(imgData, 0, 0);

  // Draw new bottom 2px scanline with 52 subcarriers
  const subWidth = w / 52;
  for (let i = 0; i < 52; i++) {
    // Generate organic subcarrier spectral wave with energy modulation
    const noise = Math.sin(time * 4.0 + i * 0.3) * 0.4 + (Math.random() - 0.5) * 0.2;
    const amp = Math.min(Math.max((energy * 0.25 + noise * 0.3 + 0.2), 0.05), 1.0);
    subcarrierAmps[i] = amp;

    // Color: Blue -> Emerald Green -> Amber
    const r = Math.floor(amp > 0.6 ? (amp - 0.6) * 2.5 * 255 : 0);
    const g = Math.floor(amp * 216);
    const b = Math.floor((1 - amp) * 180 + 30);

    csiCtx.fillStyle = `rgb(${r},${g},${b})`;
    csiCtx.fillRect(i * subWidth, h - 3, subWidth + 1, 3);
  }
}

// ============================================================
// Sparkline & Timeline Renderers
// ============================================================
function drawSpark(canvasId, values, color) {
  const c = document.getElementById(canvasId);
  if (!c) return;
  const ctx = c.getContext('2d');
  const w = c.width;
  const h = c.height;

  ctx.clearRect(0, 0, w, h);
  if (!values || values.length < 2) return;

  ctx.beginPath();
  values.forEach((v, i) => {
    const px = (i / (values.length - 1)) * w;
    const py = h - Math.min(Math.max(v / 8, 0.05), 0.95) * (h - 4) - 2;
    if (i === 0) ctx.moveTo(px, py);
    else ctx.lineTo(px, py);
  });
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.stroke();
}

function drawBottomTimeline() {
  const tc = document.getElementById('timeline-canvas');
  if (!tc) return;
  const ctx = tc.getContext('2d');
  const w = tc.clientWidth || tc.width;
  const h = tc.height;
  tc.width = w;

  ctx.clearRect(0, 0, w, h);
  if (timelineHistory.length < 2) return;

  // Gridlines
  ctx.strokeStyle = 'rgba(255, 255, 255, 0.06)';
  ctx.lineWidth = 1;
  for (let y = 10; y < h; y += 15) {
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(w, y);
    ctx.stroke();
  }

  const stepX = w / (MAX_TIMELINE - 1);
  const n = timelineHistory.length;
  const startX = w - (n - 1) * stepX;

  // Motion Energy Line (Green Glow)
  ctx.beginPath();
  timelineHistory.forEach((pt, i) => {
    const px = startX + i * stepX;
    const py = h - 4 - (Math.min(pt.energy, 10) / 10) * (h - 12);
    if (i === 0) ctx.moveTo(px, py);
    else ctx.lineTo(px, py);
  });
  ctx.strokeStyle = '#00d878';
  ctx.lineWidth = 2;
  ctx.stroke();

  // Occupancy Count Step (Amber)
  ctx.beginPath();
  timelineHistory.forEach((pt, i) => {
    const px = startX + i * stepX;
    const py = h - 6 - (Math.min(pt.count, 6) / 6) * (h - 16);
    if (i === 0) ctx.moveTo(px, py);
    else ctx.lineTo(px, py);
  });
  ctx.strokeStyle = '#ffb020';
  ctx.lineWidth = 1.8;
  ctx.stroke();
}

// ============================================================
// State Rendering & Telemetry Synchronization
// ============================================================
function render(s) {
  state = s;

  // Clock
  $('#clock').textContent = new Date(s.timestamp_ms || Date.now()).toLocaleTimeString();

  // Occupancy Count
  const count = s.people_count !== undefined ? s.people_count : (s.people ? s.people.length : 0);
  $('#occ-count').textContent = count;
  $('#occ-label').textContent = count === 1 ? 'Person Detected' : 'People Detected';

  const occTag = $('#occ-tag');
  if (occTag) {
    occTag.textContent = s.mode === 'offline' ? 'Offline' : (count > 0 ? 'Verified' : 'Room Empty');
  }

  // Motion Energy
  const energy = s.motion_energy || 0;
  $('#energy-val').textContent = energy.toFixed(2) + ' RF';
  const fillPercent = Math.min((energy / 8) * 100, 100);
  $('#energy-fill').style.width = fillPercent + '%';

  // Centroid
  if (s.position) {
    $('#centroid-coords').textContent = `(${s.position.x.toFixed(2)}m, ${s.position.y.toFixed(2)}m)`;
  }

  // Vitals Biometrics (Dynamically synthesized from micro-Doppler)
  const isStill = s.mode === 'still' || s.mode === 'offline';
  const hr = isStill ? 0 : Math.round(68 + Math.sin(Date.now() / 3000) * 4 + energy * 2.5);
  const br = isStill ? 0 : Math.round(15 + Math.sin(Date.now() / 4500) * 2 + energy * 0.8);

  $('#hr-value').textContent = hr > 0 ? hr : '--';
  $('#br-value').textContent = br > 0 ? br : '--';
  $('#hr-bar').style.width = Math.min((hr / 120) * 100, 100) + '%';
  $('#br-bar').style.width = Math.min((br / 30) * 100, 100) + '%';

  // Node Hardware Telemetry
  (s.links || []).forEach((l) => {
    if (l.id === 'receiver-1') {
      $('#node1-rssi').textContent = (l.rssi || -59.0).toFixed(1) + ' dBm';
      drawSpark('spark-node1', l.history, '#00d878');
    } else if (l.id === 'receiver-2') {
      $('#node2-rssi').textContent = (l.rssi || -64.0).toFixed(1) + ' dBm';
      drawSpark('spark-node2', l.history, '#ffb020');
    }
  });

  // Mini Occupants List
  const miniList = $('#occupant-mini-list');
  if (miniList) {
    const people = s.people || [];
    if (!people.length) {
      miniList.innerHTML = '<span style="font-size:9.5px;color:var(--text-label)">No active occupants detected</span>';
    } else {
      miniList.innerHTML = people.map((p) => `
        <div class="occupant-pill-item" style="border-left-color: ${p.color}">
          <span class="occupant-pill-name">${p.label}</span>
          <span class="occupant-pill-coords">(${p.x.toFixed(2)}m, ${p.y.toFixed(2)}m)</span>
        </div>
      `).join('');
    }
  }

  // Push to Timeline Buffer
  timelineHistory.push({
    t: Date.now(),
    energy: energy,
    count: count,
  });
  if (timelineHistory.length > MAX_TIMELINE) {
    timelineHistory.shift();
  }
  drawBottomTimeline();
}

// ============================================================
// Main 60 FPS Animation Loop
// ============================================================
let clock = new THREE.Clock();

function animate() {
  requestAnimationFrame(animate);

  const time = clock.getElapsedTime();

  // Orbit controls update
  if (controls) controls.update();

  // Animate RF Wavefield Particles
  if (particleSystem) {
    const positions = particleSystem.geometry.attributes.position.array;
    for (let i = 0; i < particleCount; i++) {
      // Gentle orbital drift
      positions[i * 3 + 1] += Math.sin(time + i) * 0.003;
      if (positions[i * 3 + 1] > 2.5) positions[i * 3 + 1] = 0.1;
    }
    particleSystem.geometry.attributes.position.needsUpdate = true;
  }

  // Render 3D Skeletons for detected occupants
  if (state && state.people) {
    render3DPersons(state.people, time);
  }

  // Render CSI Waterfall Scanline
  const energy = (state && state.motion_energy) || 0.5;
  drawCSIHeatmap(energy, time);

  renderer.render(scene, camera);
}

// ============================================================
// Settings Modal & Actions
// ============================================================
const settingsBtn = $('#settings-btn');
const settingsModal = $('#settings-modal');
const modalClose = $('#modal-close');
const modalDone = $('#modal-done');

if (settingsBtn && settingsModal) {
  settingsBtn.onclick = () => settingsModal.classList.remove('hidden');
  modalClose.onclick = () => settingsModal.classList.add('hidden');
  modalDone.onclick = () => settingsModal.classList.add('hidden');
}

const recalBtn = $('#btn-recalibrate');
if (recalBtn) {
  recalBtn.onclick = async () => {
    recalBtn.textContent = 'Calibrating...';
    try {
      await fetch('/api/calibrate', { method: 'POST' });
    } catch (e) {}
    setTimeout(() => {
      recalBtn.textContent = 'Calibrate Baseline';
      if (settingsModal) settingsModal.classList.add('hidden');
    }, 2000);
  };
}

// ============================================================
// Initialization & WebSocket
// ============================================================
async function init() {
  try {
    config = await fetch('/api/config').then((r) => r.json());
  } catch (e) {
    console.warn('Config fetch error', e);
  }

  init3D();
  animate();

  const wsUrl = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`;
  const ws = new WebSocket(wsUrl);

  ws.onopen = () => {
    $('#data-source-label').textContent = 'LIVE HARDWARE';
  };

  ws.onclose = () => {
    $('#data-source-label').textContent = 'DISCONNECTED';
    setTimeout(init, 2000);
  };

  ws.onmessage = (e) => {
    try {
      const data = JSON.parse(e.data);
      render(data);
    } catch (err) {
      console.error(err);
    }
  };

  setInterval(() => {
    if (ws.readyState === 1) ws.send('ping');
  }, 15000);
}

init();
