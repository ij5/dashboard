import '@xterm/xterm/css/xterm.css'
import './style.css'
import { Terminal } from '@xterm/xterm'
import { Unicode11Addon } from '@xterm/addon-unicode11';
// import { WebglAddon } from '@xterm/addon-webgl';
import { CanvasAddon } from '@xterm/addon-canvas';

const term = new Terminal({
  fontSize: 16,
  allowProposedApi: true,
});
// term.loadAddon(new WebglAddon());
term.loadAddon(new CanvasAddon());
term.loadAddon(new Unicode11Addon());

term.unicode.activeVersion = "11";
term.open(document.getElementById('app')!);
term.options.disableStdin = true;
term.options.scrollOnUserInput = false;

function createWebsocket() {
  return new WebSocket(
    import.meta.env.PROD ? `wss://${import.meta.env.VITE_API}/ws` : `ws://${import.meta.env.VITE_API}/ws`
  );
}

let websocket = createWebsocket();

websocket.binaryType = 'arraybuffer';

function setupWebsocket() {
  websocket.onclose = async () => {
    term.clear();
    await new Promise(res => {
      setTimeout(res, 1000);
    });
    websocket = createWebsocket();
    setupWebsocket();
  }

  websocket.onopen = () => {
    term.clear();
  }

  websocket.onmessage = (data) => {
    let raw = new Uint8Array(data.data);
    const cmd = raw[0];
    raw = raw.slice(1);
    if (cmd === 0) {
      term.write(raw);
    } else if (cmd === 1) {
      term.reset();
      term.write(raw);
    } else if (cmd === 2) {
      let source = JSON.parse(new TextDecoder().decode(raw));
      term.clear();
      term.resize(source.rows, source.cols);
    }
  }

}

setupWebsocket();
