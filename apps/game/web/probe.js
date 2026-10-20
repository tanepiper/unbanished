// window.unbanished: look at and drive the running game from tests or the browser's console
// (src/probe.rs). Every method returns a Promise of the answer, for example:
//   await unbanished.ui('kind:button')      await unbanished.click('buttonNew')
//   await unbanished.town()                  await unbanished.buildings('house')
// The game answers between frames, so it must be running. unbanished.help() lists it all.
(() => {
  function probe(command, timeout = 10000) {
    const b = window.wasmBindings;
    if (!b || !b.unbanishedProbe) return Promise.reject(new Error('the game is not loaded'));
    const id = b.unbanishedProbe(command);
    const end = Date.now() + timeout;
    return new Promise((resolve, reject) => {
      const poll = () => {
        const r = b.unbanishedProbeResult(id);
        if (r !== undefined) {
          const v = JSON.parse(r);
          return 'error' in v ? reject(new Error(v.error)) : resolve(v.ok);
        }
        if (Date.now() > end) return reject(new Error(`${command}: no answer (is the game running?)`));
        setTimeout(poll, 16);
      };
      poll();
    });
  }
  const filtered = (verb) => (filter = '') => probe(`${verb} ${filter}`.trim());
  window.unbanished = {
    probe,
    state: () => probe('state'),
    ui: (query) => probe(`ui ${query}`),
    click: (query) => probe(`click ${query}`),
    type: (name, text) => probe(`type ${name}=${text}`),
    combo: (name, item) => probe(`combo ${name}=${item}`),
    script: (command) => probe(`script ${command}`),
    town: () => probe('town'),
    stock: () => probe('stock'),
    buildings: filtered('buildings'),
    citizens: filtered('citizens'),
    animals: filtered('animals'),
    help() {
      console.log([
        'unbanished.state()              the current loop and open dialogs',
        'unbanished.ui(query)            shown elements: name, text:Exit, kind:button, ref:Options, *',
        "unbanished.click(query)         click the first usable match, e.g. click('ref:PauseMenu buttonExit')",
        'unbanished.type(name, text)     type into an edit;  unbanished.combo(name, item)',
        'unbanished.town() / stock()     the town\'s numbers and its stock',
        'unbanished.buildings(filter?)   also citizens(filter?) and animals(filter?)',
        "unbanished.script(command)      a script command: 'speed 10', 'pause', 'place TEMPLATE'",
        'unbanished.probe(command)       any probe command as text',
      ].join('\n'));
    },
  };
})();
