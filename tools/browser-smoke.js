// Paste into Chrome DevTools on the locally served viewer, before pressing Play.
// Development diagnostics only; this file is not shipped with the viewer.
(() => {
  const contexts = [];
  const sources = [];
  const NativeAudioContext = window.AudioContext;
  window.AudioContext = new Proxy(NativeAudioContext, {
    construct(Target, args) {
      const context = new Target(...args);
      contexts.push(context);
      const create = context.createBufferSource.bind(context);
      context.createBufferSource = () => {
        const source = create();
        const record = { source, context, started: 0, stopped: 0, ended: 0 };
        sources.push(record);
        source.addEventListener('ended', () => record.ended++);
        for (const method of ['start', 'stop']) {
          const original = source[method].bind(source);
          source[method] = (...args) => {
            record[method === 'start' ? 'started' : 'stopped']++;
            return original(...args);
          };
        }
        return source;
      };
      return context;
    },
  });

  window.browserSmoke = {
    // Feeds real browser File objects through the picker change handler, bypassing
    // the query-string loader. Use the native chooser separately to check its UI.
    async selectPackages(names = ['patch1.pkg', 'data0.pkg', 'patch0.pkg', 'data1.pkg']) {
      const transfer = new DataTransfer();
      for (const name of names) {
        const response = await fetch('/game-data/original/WinData/' + name);
        if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
        transfer.items.add(new File([await response.arrayBuffer()], name));
      }
      const input = document.querySelector('#packages');
      const selected = [...transfer.files].map(({ name, size }) => ({ name, size }));
      input.files = transfer.files;
      input.dispatchEvent(new Event('change', { bubbles: true }));
      return selected;
    },

    audio() {
      return {
        contexts: contexts.map(c => ({ state: c.state, time: c.currentTime })),
        sources: sources.map(record => {
          const buffer = record.source.buffer;
          const channels = [];
          for (let ch = 0; ch < (buffer?.numberOfChannels ?? 0); ch++) {
            const plane = buffer.getChannelData(ch);
            let squares = 0;
            let peak = 0;
            for (const sample of plane) {
              squares += sample * sample;
              peak = Math.max(peak, Math.abs(sample));
            }
            channels.push({ rms: Math.sqrt(squares / plane.length), peak });
          }
          return {
            started: record.started, stopped: record.stopped, ended: record.ended,
            looping: record.source.loop, frames: buffer?.length,
            sampleRate: buffer?.sampleRate, duration: buffer?.duration, channels,
          };
        }),
      };
    },

    // Start while a sound is playing, press Stop during the sampling interval,
    // then verify nonzero early RMS followed by zero. The analyser's extra output
    // is muted so the existing source-to-speakers connection remains unchanged.
    async sampleOutput(count = 20, intervalMs = 100) {
      const record = sources.at(-1);
      if (!record) throw new Error('Press Play first');
      const analyser = record.context.createAnalyser();
      const mute = record.context.createGain();
      mute.gain.value = 0;
      record.source.connect(analyser);
      analyser.connect(mute).connect(record.context.destination);
      const samples = new Float32Array(analyser.fftSize);
      const output = [];
      try {
        for (let i = 0; i < count; i++) {
          await new Promise(resolve => setTimeout(resolve, intervalMs));
          analyser.getFloatTimeDomainData(samples);
          output.push({ time: record.context.currentTime,
            rms: Math.sqrt(samples.reduce((sum, x) => sum + x * x, 0) / samples.length) });
        }
      } finally {
        // Stop disconnects the source as well; tolerate that during cleanup.
        try { record.source.disconnect(analyser); } catch (_) { /* already stopped */ }
        analyser.disconnect();
        mute.disconnect();
      }
      return output;
    },

    restore() { window.AudioContext = NativeAudioContext; },
  };
})();
