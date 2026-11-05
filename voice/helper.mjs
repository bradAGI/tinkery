import { readFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const RATE = 16000, CHUNK = 8000, MAX_SAMPLES = RATE * 300, MAX_QUEUE = 8;
const MAX_LINE = 32768;
const permissionHelp = 'Allow microphone access for your terminal in System Settings → Privacy & Security → Microphone.';
const issue = (kind, message) => Object.assign(new Error(message), { kind });
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const emit = event => {
  const line = JSON.stringify(event);
  if (Buffer.byteLength(line) > MAX_LINE) throw issue('protocol', 'Voice response exceeds the protocol limit. Try a shorter take.');
  if (process.stdout.writableLength > MAX_LINE * 4) throw issue('protocol', 'Voice output backlog exceeded its limit. Restart think.');
  process.stdout.write(line + '\n');
};
const failEvent = (error, take) => emit({ event: 'error', ...(take ? { take: take.id } : {}),
  kind: error.kind ?? 'transcription', message: error.message ?? String(error) });

function wavPcm(buffer) {
  if (buffer.toString('ascii', 0, 4) !== 'RIFF' || buffer.toString('ascii', 8, 12) !== 'WAVE')
    throw issue('capture', 'Voice test input must be a WAV.');
  let format, data;
  for (let i = 12; i + 8 <= buffer.length;) {
    const size = buffer.readUInt32LE(i + 4), end = i + 8 + size;
    if (end > buffer.length) throw issue('capture', 'Truncated voice test WAV.');
    const name = buffer.toString('ascii', i, i + 4);
    if (name === 'fmt ') format = buffer.subarray(i + 8, end);
    if (name === 'data') data = buffer.subarray(i + 8, end);
    i = end + (size % 2);
  }
  if (!format || format.length < 16 || !data?.length || format.readUInt16LE(0) !== 1 ||
      format.readUInt16LE(2) !== 1 || format.readUInt32LE(4) !== RATE || format.readUInt16LE(14) !== 16 || data.length % 2)
    throw issue('capture', 'Voice test WAV must be 16 kHz mono signed 16-bit PCM.');
  if (data.length / 2 > MAX_SAMPLES) throw issue('capture', 'Voice take exceeds five minutes.');
  return Float32Array.from({ length: data.length / 2 }, (_, i) => data.readInt16LE(i * 2) / 32768);
}

async function permission() {
  if (process.platform !== 'darwin') return;
  const script = "ObjC.import('Foundation'); ObjC.import('AVFoundation'); $.NSClassFromString('AVCaptureDevice').authorizationStatusForMediaType($.AVMediaTypeAudio).toString();";
  let stdout;
  try { ({ stdout } = await promisify(execFile)('osascript', ['-l', 'JavaScript', '-e', script], { timeout: 5000, maxBuffer: 1024 })); }
  catch { throw issue('mic-permission', permissionHelp); }
  const status = stdout.trim();
  if (status === '1' || status === '2') throw issue('mic-permission', permissionHelp);
  if (status !== '0' && status !== '3') throw issue('mic-permission', permissionHelp);
  // Undetermined is allowed to request access at the explicit start, never at load.
}

async function load() {
  const fake = process.env.TINKERY_VOICE_FAKE_PCM;
  const smoke = process.env.TINKERY_VOICE_SMOKE_WAV;
  let pcm;
  if (fake || smoke) {
    const input = fake || smoke;
    if (input === 'tone' || input === 'silence')
      pcm = Float32Array.from({ length: RATE }, (_, i) => input === 'silence' ? 0 : Math.sin(i * 2 * Math.PI * 440 / RATE) * 0.1);
    else pcm = wavPcm(await readFile(input));
  }
  if (fake) {
    const config = JSON.parse(process.env.TINKERY_VOICE_FAKE_CONFIG ?? '{}');
    const testFailure = stage => { if (config.fail === stage) throw issue(config.kind ?? 'transcription', config.message ?? 'Injected voice failure.'); };
    testFailure('load');
    await sleep(config.loadDelay ?? 0);
    const stream = () => ({ feed: async pcm => {
      const before = config.verifyBorrowed ? pcm.slice() : undefined;
      await sleep(config.feedDelay ?? 0);
      if (before && before.some((value, i) => value !== pcm[i])) throw issue('transcription', 'Borrowed PCM was mutated.');
      testFailure('feed');
    },
      finalize: async () => { await sleep(config.finalDelay ?? 0); testFailure('final'); },
      get snapshot() { return { text: config.text ?? 'spoken words' }; }, reset() {} });
    const trace = operation => { if (config.traceRecorder) emit({ event: 'test-recorder', operation }); };
    class Recorder {
      static getAvailableDevices() { return config.noDevices ? [] : ['Simulated microphone']; }
      constructor() { this.sampleRate = config.sampleRate ?? RATE; this.isRecording = false; this.offset = 0; }
      start() { if (config.nativeError) throw new Error(config.nativeError); testFailure('record'); this.isRecording = true; trace('start'); }
      stop() { this.isRecording = false; trace('stop'); }
      release() { trace('release'); }
      async read() {
        await sleep(10);
        // Like PvRecorder: a read pending when stop() is called rejects.
        if (!this.isRecording) throw new Error('PvRecorder failed to read audio data frame');
        testFailure('read');
        return Int16Array.from({ length: 512 }, () => Math.round(pcm[this.offset++ % pcm.length] * 32768));
      }
    }
    return { PvRecorder: Recorder, settings: { microphone: config.microphone ?? { type: 'system-default' } }, language: 'en',
      checkPermission: async () => { await sleep(config.permissionDelay ?? 0); testFailure('permission'); },
      model: { capabilities: { supportsStreaming: !config.batch }, createSession: () => ({ stream: async () => { await sleep(config.startDelay ?? 0); testFailure('start'); return stream(); }, dispose() {} }),
        transcribe: async () => { await sleep(config.finalDelay ?? 0); testFailure('final'); return { text: config.text ?? 'spoken words' }; } } };
  }
  const { paths, config: settings, model: modelPath } = owned;
  try {
    const { engine, PvRecorder } = await loadVoiceModules(paths, !smoke);
    const model = await engine.TranscribeModel.load(modelPath);
    if (!model.capabilities.languages.includes(settings.language))
      throw new Error('Unsupported language. Check ~/.config/tinkery/voice.json.');
    return { model, PvRecorder, settings, language: settings.language, pcm, checkPermission: smoke ? async () => {} : permission };
  } catch (error) {
    throw issue('engine-load', `Voice engine could not load: ${shortReason(error)}`);
  }
}

let dependencies, current, closed = false, permissionChecked = false, permissionPromise, lastId = 0;
const live = take => current === take && !take.cancelled && !closed;
function haltRecorder(take) {
  take.stopping = true;
  if (take.recorder?.isRecording) take.recorder.stop();
}
function cancel(take) {
  if (!take) return;
  take.cancelled = true;
  if (current === take) current = undefined;
  take.abort.abort();
  try { haltRecorder(take); } catch {}
  take.stream?.reset();
  take.session?.dispose();
}
function failure(error, take) {
  if (!live(take)) return;
  cancel(take);
  failEvent(error, take);
}
function queue(take, pcm) {
  if (!pcm.length || !live(take)) return;
  if (++take.backlog > MAX_QUEUE) throw issue('capture', 'Voice audio backlog exceeded its limit. Try a shorter take.');
  const immutable = pcm.slice();
  take.feeds = take.feeds.then(async () => {
    if (!live(take)) return;
    if (take.stream) {
      await take.stream.feed(immutable);
      if (live(take)) emit({ event: 'partial', take: take.id, text: take.stream.snapshot.text });
    } else take.frames.push(immutable);
  }).catch(error => failure(error, take)).finally(() => { take.backlog--; });
}
function capture(take, frame) {
  if (!live(take) || take.stopping) return;
  if ((take.samples += frame.length) > MAX_SAMPLES) throw issue('capture', 'Voice take exceeds five minutes. Stop and send, then speak more.');
  for (const value of frame) {
    take.energy += value * value;
    take.chunk[take.used++] = value;
    if (take.used === CHUNK) { queue(take, take.chunk); take.used = 0; }
  }
}
async function read(take) {
  try {
    if (dependencies.pcm) {
      const pcm = dependencies.pcm;
      for (let offset = 0; live(take) && !take.stopping; offset += 512) {
        if (offset >= pcm.length) break;
        const start = offset % pcm.length;
        capture(take, pcm.subarray(start, Math.min(start + 512, pcm.length)));
        await sleep(10);
      }
    } else {
      while (live(take) && !take.stopping && take.recorder.isRecording) {
        let frame;
        try { frame = await take.recorder.read(); }
        catch (error) {
          // Stopping the recorder rejects the pending read: that is the end of the take, not a failure.
          if (take.stopping || !take.recorder?.isRecording) break;
          throw error;
        }
        capture(take, Float32Array.from(frame, value => value / 32768));
      }
    }
  } catch (error) { failure(error.kind ? error : issue('capture', `Microphone capture failed: ${shortReason(error)}`), take); }
  finally { take.recorder?.release(); take.recorder = undefined; }
}
function microphone() {
  let recorder;
  try {
    const devices = dependencies.PvRecorder.getAvailableDevices();
    let index = -1;
    if (dependencies.settings.microphone.type === 'device') {
      const selected = dependencies.settings.microphone;
      index = devices.map((name, i) => ({ name, i })).filter(item => item.name === selected.name)[selected.occurrence]?.i ?? -1;
      if (index < 0) throw issue('mic-unavailable', 'No microphone found.');
    } else if (!devices.length) throw issue('mic-unavailable', 'No microphone found.');
    recorder = new dependencies.PvRecorder(512, index);
    if (recorder.sampleRate !== RATE) throw issue('capture', 'Microphone must supply 16 kHz PCM.');
    recorder.start();
    return recorder;
  } catch (error) {
    recorder?.release();
    if (error.kind) throw error;
    if (/DeviceAlreadyInitialized|in use|busy/i.test(error.name + ' ' + error.message))
      throw issue('mic-busy', 'Microphone is in use by another app.');
    throw issue('capture', `Microphone could not start: ${shortReason(error)}`);
  }
}
async function begin(take) {
  try {
    if (!permissionChecked) {
      permissionPromise ??= dependencies.checkPermission().finally(() => { permissionPromise = undefined; });
      await permissionPromise; permissionChecked = true;
    }
    if (!live(take)) return;
    if (dependencies.model.capabilities.supportsStreaming) {
      take.session = dependencies.model.createSession();
      take.stream = await take.session.stream({ timestamps: 'none', ...(dependencies.language ? { language: dependencies.language } : {}) });
    }
    if (!live(take)) { take.stream?.reset(); take.session?.dispose(); return; }
    if (take.stopRequested) return;
    if (!dependencies.pcm) take.recorder = microphone();
    emit({ event: 'listening', take: take.id });
    take.reading = read(take);
  } catch (error) { take.recorder?.release(); take.recorder = undefined; failure(error, take); }
}
async function finish(take) {
  take.stopRequested = true;
  try {
    haltRecorder(take);
    await take.starting;
    await take.reading;
    if (!live(take)) return;
    if (take.used) queue(take, take.chunk.subarray(0, take.used));
    await take.feeds;
    if (!live(take)) return;
    if (take.samples && Math.sqrt(take.energy / take.samples) < 0.00001)
      throw issue('capture', 'I didn’t hear anything');
    let text = '';
    if (take.samples) {
      if (take.stream) { await take.stream.finalize(); if (live(take)) text = take.stream.snapshot.text; }
      else {
        const pcm = new Float32Array(take.samples);
        let offset = 0;
        for (const frame of take.frames) { pcm.set(frame, offset); offset += frame.length; }
        const result = await dependencies.model.transcribe(pcm, { signal: take.abort.signal, timestamps: 'none', ...(dependencies.language ? { language: dependencies.language } : {}) });
        text = result.text;
      }
    }
    if (live(take)) { emit({ event: 'final', take: take.id, text: text.trim() }); cancel(take); }
  } catch (error) { failure(error, take); }
}
function command(value) {
  if (value && ['install', 'download'].includes(value.command)) {
    if (current || setupBusy || requirement?.event !== (value.command === 'install' ? 'needsinstall' : 'needsmodel'))
      throw issue('protocol', 'Voice setup is not awaiting that action.');
    void setupAction(value.command);
    return;
  }
  if (!value || !['start', 'stop', 'cancel'].includes(value.command) || !Number.isSafeInteger(value.take) || value.take < 1)
    throw issue('protocol', 'Invalid voice command. Restart think.');
  if (value.command === 'start') {
    if (!dependencies) throw issue('protocol', 'Voice is not ready. Try again after loading.');
    if (current || value.take <= lastId) throw issue('protocol', 'Voice take is already active or stale.');
    lastId = value.take;
    const take = { id: value.take, abort: new AbortController(), chunk: new Float32Array(CHUNK), frames: [], used: 0, samples: 0,
      energy: 0, backlog: 0, feeds: Promise.resolve(), cancelled: false, stopping: false, stopRequested: false };
    current = take;
    take.starting = begin(take);
  } else if (current?.id === value.take) {
    if (value.command === 'cancel') cancel(current);
    else if (!current.stopRequested) void finish(current);
  }
}
function exit() {
  closed = true;
  cancel(current);
  // Native inference is not awaited: stdin EOF must not orphan capture or block exit.
  process.exit(0);
}
process.on('SIGTERM', exit);
process.on('SIGINT', exit);
process.stdin.on('end', exit);
process.stdin.on('error', exit);
let input = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', chunk => {
  try {
    input += chunk;
    for (;;) {
      const end = input.indexOf('\n');
      if (end < 0) { if (Buffer.byteLength(input) > MAX_LINE) throw issue('protocol', 'Voice command exceeds its limit.'); break; }
      const line = input.slice(0, end); input = input.slice(end + 1);
      if (Buffer.byteLength(line) > MAX_LINE) throw issue('protocol', 'Voice command exceeds its limit.');
      command(JSON.parse(line));
    }
  } catch (error) {
    input = '';
    const take = current;
    cancel(take);
    failEvent(error.kind ? error : issue('protocol', 'Malformed voice command. Restart think.'), take);
  }
});
let owned, requirement, setupBusy = false;
const fakeConfig = process.env.TINKERY_VOICE_FAKE_PCM ? JSON.parse(process.env.TINKERY_VOICE_FAKE_CONFIG ?? '{}') : undefined;
let fakeRequirement = fakeConfig?.setup;
async function prepareVoice() {
  dependencies = undefined;
  if (fakeConfig && fakeRequirement) {
    requirement = { event: fakeRequirement === 'model' ? 'needsmodel' : 'needsinstall', size_mb: 20,
      message: fakeRequirement === 'model' ? 'Voice model is missing (732 MB). Download now? y / n' : 'Voice needs a one-time local install (about 20 MB). Install now? y / n' };
  } else if (!fakeConfig) {
    owned = await discoverVoice(); requirement = owned.requirement;
  } else requirement = undefined;
  if (requirement) { emit(requirement); return; }
  emit({ event: 'loading' });
  dependencies = await load();
  if (!closed) emit({ event: 'ready', version: 1 });
}
async function setupAction(action) {
  setupBusy = true;
  try {
    if (fakeConfig) {
      await sleep(fakeConfig.installDelay ?? 0);
      if (fakeConfig.installFailure) throw new Error(fakeConfig.installFailure);
      fakeRequirement = undefined;
    } else if (action === 'install') await installVoice(voicePaths());
    else await downloadVoiceModel(voicePaths());
    if (!closed) await prepareVoice();
  } catch (error) {
    failEvent(issue(action === 'install' ? 'install' : 'download', `Voice ${action === 'install' ? 'install' : 'download'} failed: ${shortReason(error)}. Press the voice key to retry.`));
  } finally { setupBusy = false; }
}
try { await prepareVoice(); }
catch (error) { failEvent(error); }
