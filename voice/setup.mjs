import * as setupFs from 'node:fs/promises';
import { homedir as setupHome } from 'node:os';
import { join as setupJoin, dirname as setupDirname, resolve as setupResolve } from 'node:path';
import { spawn as setupSpawn } from 'node:child_process';
import { createRequire as setupRequire } from 'node:module';
import { pathToFileURL as setupFileURL } from 'node:url';

export const PACKAGES = { 'transcribe-cpp': '0.2.2', '@picovoice/pvrecorder-node': '1.2.9' };
export const MODEL_FILE = 'parakeet-unified-en-0.6b-Q8_0.gguf';
const MODEL_BYTES = 731357568;
const modelUrl = `https://huggingface.co/handy-computer/parakeet-unified-en-0.6b-gguf/resolve/main/${MODEL_FILE}`;
const setupIssue = (kind, message) => Object.assign(new Error(message), { kind });
export const shortReason = error => Array.from(String(error.message ?? error).replace(/[\x00-\x1f\x7f]/g, ' ')).slice(0, 100).join('').trim();
export function voicePaths(home = setupHome(), env = process.env) {
  const root = env.TINKERY_VOICE_ROOT ? setupResolve(env.TINKERY_VOICE_ROOT) : setupJoin(home, '.local/share/tinkery/voice');
  return { root, metadata: setupJoin(root, 'installation.json'), config: setupJoin(home, '.config/tinkery/voice.json'),
    cache: setupJoin(home, '.cache/huggingface/hub/models--handy-computer--parakeet-unified-en-0.6b-gguf/snapshots'),
    downloaded: setupJoin(root, 'models', MODEL_FILE) };
}
function installMegabytes(platform = process.platform) {
  return platform === 'darwin' ? 20 : platform === 'linux' ? 90 : 85;
}
async function setupExists(path) {
  try { return (await setupFs.stat(path)).isFile(); }
  catch (error) { if (error.code === 'ENOENT') return false; throw error; }
}
export async function readVoiceConfig(paths) {
  let config;
  try { config = JSON.parse(await setupFs.readFile(paths.config, 'utf8')); }
  catch (error) { if (error.code === 'ENOENT') return { microphone: { type: 'system-default' }, language: 'en' }; throw setupIssue('config', 'Check ~/.config/tinkery/voice.json: invalid or unreadable JSON.'); }
  if (!config || typeof config !== 'object' || Array.isArray(config) || Object.keys(config).some(key => !['microphone', 'language', 'model'].includes(key)) ||
      (config.language !== undefined && (typeof config.language !== 'string' || !config.language.trim())) ||
      (config.model !== undefined && (typeof config.model !== 'string' || !config.model.trim())) ||
      (config.microphone !== undefined && (typeof config.microphone !== 'string' || !config.microphone.trim())))
    throw setupIssue('config', 'Check ~/.config/tinkery/voice.json: microphone, language and model must be nonempty strings.');
  return { microphone: config.microphone && config.microphone !== 'system-default' ? { type: 'device', name: config.microphone, occurrence: 0 } : { type: 'system-default' },
    language: config.language ?? 'en', model: config.model };
}
export async function findVoiceModel(paths, config, env = process.env) {
  const override = env.TINKERY_VOICE_MODEL ?? config.model;
  if (override !== undefined) {
    if (!override.trim() || !await setupExists(override)) throw setupIssue('no-model', env.TINKERY_VOICE_MODEL !== undefined ? 'Voice model not found. Check TINKERY_VOICE_MODEL.' : 'Voice model not found. Check model in ~/.config/tinkery/voice.json.');
    return override;
  }
  let snapshots;
  try { snapshots = await setupFs.readdir(paths.cache); }
  catch (error) { if (error.code !== 'ENOENT') throw error; snapshots = []; }
  const candidates = [];
  for (const snapshot of snapshots) {
    const directory = setupJoin(paths.cache, snapshot), path = setupJoin(directory, MODEL_FILE);
    if (await setupExists(path)) candidates.push({ path, modified: (await setupFs.stat(directory)).mtimeMs });
  }
  candidates.sort((a, b) => b.modified - a.modified || a.path.localeCompare(b.path));
  return candidates[0]?.path ?? (await setupExists(paths.downloaded) ? paths.downloaded : undefined);
}
export async function installationRequirement(paths, major = Number(process.versions.node.split('.')[0])) {
  let metadata;
  try { metadata = JSON.parse(await setupFs.readFile(paths.metadata, 'utf8')); }
  catch (error) { if (!['ENOENT', 'SyntaxError'].includes(error.code ?? error.name)) throw error; }
  const size = installMegabytes();
  const message = `Voice needs a one-time local install (about ${size} MB). Install now? y / n`;
  if (!metadata || metadata.version !== 1 || !Number.isInteger(metadata.nodeMajor)) return { event: 'needsinstall', size_mb: size, message };
  if (metadata.nodeMajor !== major) return { event: 'needsinstall', size_mb: size, message: `Voice used Node ${metadata.nodeMajor}; found ${major}. Reinstall (~${size} MB)? y / n` };
  for (const [name, version] of Object.entries(PACKAGES)) {
    let installed;
    try { installed = JSON.parse(await setupFs.readFile(setupJoin(paths.root, 'node_modules', name, 'package.json'), 'utf8')); }
    catch (error) { if (!['ENOENT', 'SyntaxError'].includes(error.code ?? error.name)) throw error; }
    if (installed?.version !== version) return { event: 'needsinstall', size_mb: size, message };
  }
}
export async function runVoiceNpm(paths, executable = process.execPath) {
  let cli;
  for (const candidate of [setupJoin(setupDirname(executable), 'npm'), setupJoin(setupDirname(executable), 'node_modules/npm/bin/npm-cli.js')]) {
    try { cli = await setupFs.realpath(candidate); break; } catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  if (!cli) throw new Error('npm is missing for this Node. Install npm or set TINKERY_NODE.');
  await new Promise((resolve, reject) => {
    const child = setupSpawn(executable, [cli, 'install', '--ignore-scripts', '--no-audit', '--no-fund', '--save-exact', ...Object.entries(PACKAGES).map(([name, version]) => `${name}@${version}`)],
      { cwd: paths.root, stdio: ['ignore', 'pipe', 'pipe'] });
    let output = '';
    const collect = chunk => { output = (output + chunk.toString()).slice(-4096); };
    child.stdout.on('data', collect); child.stderr.on('data', collect);
    child.on('error', reject);
    child.on('close', code => code === 0 ? resolve() : reject(new Error(output.split('\n').filter(line => /error/i.test(line)).pop() ?? `npm exited ${code}`)));
  });
}
export async function loadVoiceModules(paths, recorder = true) {
  const engine = await import(setupFileURL(setupJoin(paths.root, 'node_modules/transcribe-cpp/dist/index.js')).href);
  engine.setLogHandler(() => {});
  const require = setupRequire(setupJoin(paths.root, 'package.json'));
  const { PvRecorder } = recorder ? require(setupJoin(paths.root, 'node_modules/@picovoice/pvrecorder-node')) : {};
  return { engine, PvRecorder };
}
export async function installVoice(paths, run = runVoiceNpm, verify = loadVoiceModules) {
  await setupFs.mkdir(paths.root, { recursive: true });
  await setupFs.writeFile(setupJoin(paths.root, 'package.json'), JSON.stringify({ name: 'tinkery-local-voice', private: true, dependencies: PACKAGES }) + '\n');
  try { await setupFs.unlink(paths.metadata); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  await run(paths);
  await verify(paths);
  const metadata = { version: 1, nodeMajor: Number(process.versions.node.split('.')[0]), packages: PACKAGES };
  const temporary = paths.metadata + '.tmp';
  await setupFs.writeFile(temporary, JSON.stringify(metadata) + '\n');
  await setupFs.rename(temporary, paths.metadata);
}
export async function discoverVoice(paths = voicePaths(), env = process.env) {
  const requirement = await installationRequirement(paths);
  if (requirement) return { requirement };
  const config = await readVoiceConfig(paths), model = await findVoiceModel(paths, config, env);
  if (!model) return { requirement: { event: 'needsmodel', size_mb: Math.ceil(MODEL_BYTES / 1000000), message: `Voice model is missing (${Math.ceil(MODEL_BYTES / 1000000)} MB). Download now? y / n` } };
  return { paths, config, model };
}
export async function downloadVoiceModel(paths, fetchModel = fetch, expectedBytes = MODEL_BYTES) {
  await setupFs.mkdir(setupDirname(paths.downloaded), { recursive: true });
  const temporary = paths.downloaded + '.part';
  let handle;
  try {
    if (await setupExists(paths.downloaded)) return;
    const response = await fetchModel(modelUrl, { signal: AbortSignal.timeout(540000) });
    if (!response.ok || !response.body) throw new Error(`model server returned ${response.status}`);
    const length = response.headers.get('content-length');
    if (length !== null && Number(length) !== expectedBytes) throw new Error('model download size changed; no model installed');
    handle = await setupFs.open(temporary, 'w');
    let bytes = 0;
    for await (const chunk of response.body) {
      bytes += chunk.length;
      if (bytes > expectedBytes) throw new Error('model download exceeded its expected size');
      await handle.writeFile(chunk);
    }
    if (bytes !== expectedBytes) throw new Error('model download was incomplete');
    await handle.close(); handle = undefined;
    await setupFs.rename(temporary, paths.downloaded);
  } catch (error) {
    await handle?.close();
    await setupFs.unlink(temporary).catch(() => {});
    throw error;
  }
}
