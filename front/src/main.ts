import '@xterm/xterm/css/xterm.css'
import './style.css'
import { Terminal } from '@xterm/xterm'
import { WebglAddon } from '@xterm/addon-webgl';

let websocket = new WebSocket(
  import.meta.env.PROD ? `wss://${location.hostname}/ws` : `ws://${import.meta.env.VITE_API}/ws`
)

websocket.binaryType = 'arraybuffer';

const term = new Terminal();
term.loadAddon(new WebglAddon());

websocket.onclose = () => {
}

websocket.onopen = () => {
  term.open(document.getElementById('app')!);
  term.resize(160, 45);
}

websocket.onmessage = (data) => {
  let raw = new Uint8Array(data.data);
  const cmd = raw[0];
  raw = raw.slice(1);
  if (cmd === 0) {
    term.write(raw);
  } else if (cmd === 1) {
    let source = JSON.parse(new TextDecoder().decode(raw));
    console.log(source)
    term.resize(source.cols, source.rows);
  }
}
