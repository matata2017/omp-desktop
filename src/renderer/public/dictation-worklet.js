/**
 * Dictation capture: runs on the audio thread of a 16 kHz AudioContext (Chromium
 * resamples the microphone to the context rate), batches mono samples into 100 ms
 * PCM16 chunks, and posts each with a 0–1 loudness level for the composer meter.
 * Loaded with `audioWorklet.addModule` from the renderer origin (CSP `script-src 'self'`).
 */
const CHUNK_FRAMES = 1600;

class DictationCapture extends AudioWorkletProcessor {
	constructor() {
		super();
		this.chunk = new Int16Array(CHUNK_FRAMES);
		this.filled = 0;
		this.sumSquares = 0;
	}

	process(inputs) {
		const channel = inputs[0]?.[0];
		if (!channel) return true;
		for (let i = 0; i < channel.length; i++) {
			const sample = Math.max(-1, Math.min(1, channel[i]));
			this.sumSquares += sample * sample;
			this.chunk[this.filled++] = sample < 0 ? sample * 0x8000 : sample * 0x7fff;
			if (this.filled === CHUNK_FRAMES) this.flush();
		}
		return true;
	}

	flush() {
		const rms = Math.sqrt(this.sumSquares / CHUNK_FRAMES);
		// -60 dBFS (room tone) .. 0 dBFS (clipping) mapped onto 0..1.
		const level = rms > 0 ? Math.max(0, Math.min(1, (20 * Math.log10(rms) + 60) / 60)) : 0;
		const pcm = this.chunk;
		this.port.postMessage({ pcm: pcm.buffer, level }, [pcm.buffer]);
		this.chunk = new Int16Array(CHUNK_FRAMES);
		this.filled = 0;
		this.sumSquares = 0;
	}
}

registerProcessor("dictation-capture", DictationCapture);
