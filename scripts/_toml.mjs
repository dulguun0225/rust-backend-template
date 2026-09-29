// A TOML reader for the files the gates read: Cargo.toml, clippy.toml, deny.toml, layering.toml,
// clippy-scopes.toml, table-owners.toml, rust-toolchain.toml, .cargo/config.toml. It covers tables, arrays of
// tables, dotted and quoted keys, basic and literal strings (single- and multi-line), integers, floats,
// booleans, arrays and inline tables. Dates are read as strings. Anything else is a parse error naming the
// line: a gate that cannot read its input fails rather than reading it wrong.
import { Fail } from './_lib.mjs';

export function parseToml(text, file = '<toml>') {
  const src = text.replace(/\r\n/g, '\n');
  let i = 0;
  let line = 1;
  const root = {};
  let current = root;

  const err = (msg) => new Fail(`${file}:${line}: ${msg}`);
  const peek = () => src[i];
  const next = () => {
    const c = src[i++];
    if (c === '\n') line += 1;
    return c;
  };
  const skipWs = () => {
    while (i < src.length && (src[i] === ' ' || src[i] === '\t')) i += 1;
  };
  const skipComment = () => {
    if (src[i] === '#') while (i < src.length && src[i] !== '\n') i += 1;
  };
  const skipWsCommentsNewlines = () => {
    for (;;) {
      skipWs();
      skipComment();
      if (src[i] === '\n') next();
      else break;
    }
  };
  const expectEol = () => {
    skipWs();
    skipComment();
    if (i < src.length && src[i] !== '\n') throw err(`unexpected ${JSON.stringify(src[i])}`);
    if (i < src.length) next();
  };

  const bareKey = /[A-Za-z0-9_-]/;
  function key() {
    skipWs();
    if (src[i] === '"' || src[i] === "'") return string();
    let k = '';
    while (i < src.length && bareKey.test(src[i])) k += src[i++];
    if (k === '') throw err('expected a key');
    return k;
  }
  function dottedKey() {
    const parts = [key()];
    for (;;) {
      skipWs();
      if (src[i] !== '.') return parts;
      i += 1;
      parts.push(key());
    }
  }

  function string() {
    const quote = src[i];
    const triple = src.startsWith(quote.repeat(3), i);
    if (triple) {
      i += 3;
      if (src[i] === '\n') next();
      let s = '';
      for (;;) {
        if (i >= src.length) throw err('unterminated string');
        if (src.startsWith(quote.repeat(3), i)) {
          i += 3;
          while (src[i] === quote) s += src[i++];
          return s;
        }
        if (quote === '"' && src[i] === '\\') s += escape(true);
        else s += next();
      }
    }
    i += 1;
    let s = '';
    for (;;) {
      if (i >= src.length || src[i] === '\n') throw err('unterminated string');
      if (src[i] === quote) {
        i += 1;
        return s;
      }
      if (quote === '"' && src[i] === '\\') s += escape(false);
      else s += src[i++];
    }
  }
  function escape(multiline) {
    i += 1;
    const c = src[i++];
    const simple = { b: '\b', t: '\t', n: '\n', f: '\f', r: '\r', '"': '"', '\\': '\\', e: '\x1b' };
    if (c in simple) return simple[c];
    if (c === 'u' || c === 'U') {
      const n = c === 'u' ? 4 : 8;
      const hex = src.slice(i, i + n);
      if (!/^[0-9a-fA-F]+$/.test(hex) || hex.length !== n) throw err('bad unicode escape');
      i += n;
      return String.fromCodePoint(parseInt(hex, 16));
    }
    if (multiline && (c === '\n' || c === ' ' || c === '\t')) {
      i -= 1;
      while (i < src.length && /[ \t\n]/.test(src[i])) next();
      return '';
    }
    throw err(`bad escape \\${c}`);
  }

  function value() {
    skipWs();
    const c = src[i];
    if (c === '"' || c === "'") return string();
    if (c === '[') return array();
    if (c === '{') return inlineTable();
    const m = /^(true|false|[+-]?(?:inf|nan)|[0-9+\-.eE_:TZxob]+[0-9A-Za-z+\-.:_]*)/.exec(src.slice(i));
    if (!m) throw err(`expected a value, found ${JSON.stringify(c)}`);
    i += m[0].length;
    const t = m[0];
    if (t === 'true') return true;
    if (t === 'false') return false;
    if (/^[+-]?\d[\d_]*$/.test(t)) return Number(t.replaceAll('_', ''));
    if (/^0x[0-9a-fA-F_]+$/.test(t)) return parseInt(t.slice(2).replaceAll('_', ''), 16);
    if (/^[+-]?\d[\d_]*(\.\d[\d_]*)?([eE][+-]?\d+)?$/.test(t)) return Number(t.replaceAll('_', ''));
    if (/^\d{4}-\d{2}-\d{2}/.test(t) || /^\d{2}:\d{2}/.test(t)) return t;
    throw err(`unsupported value ${t}`);
  }
  function array() {
    i += 1;
    const out = [];
    for (;;) {
      skipWsCommentsNewlines();
      if (src[i] === ']') {
        i += 1;
        return out;
      }
      out.push(value());
      skipWsCommentsNewlines();
      if (src[i] === ',') {
        i += 1;
        continue;
      }
      if (src[i] === ']') {
        i += 1;
        return out;
      }
      throw err('expected , or ] in array');
    }
  }
  function inlineTable() {
    i += 1;
    const out = {};
    skipWs();
    if (src[i] === '}') {
      i += 1;
      return out;
    }
    for (;;) {
      const k = dottedKey();
      skipWs();
      if (src[i] !== '=') throw err('expected = in inline table');
      i += 1;
      assign(out, k, value());
      skipWs();
      if (src[i] === ',') {
        i += 1;
        skipWs();
        if (src[i] === '}') {
          i += 1;
          return out;
        }
        continue;
      }
      if (src[i] === '}') {
        i += 1;
        return out;
      }
      throw err('expected , or } in inline table');
    }
  }

  function descend(table, k) {
    if (!(k in table)) table[k] = {};
    const v = table[k];
    if (Array.isArray(v)) return v[v.length - 1];
    if (typeof v !== 'object' || v === null) throw err(`key ${k} is not a table`);
    return v;
  }
  function assign(table, parts, v) {
    let t = table;
    for (const p of parts.slice(0, -1)) t = descend(t, p);
    const last = parts[parts.length - 1];
    if (last in t) throw err(`duplicate key ${parts.join('.')}`);
    t[last] = v;
  }

  while (i < src.length) {
    skipWsCommentsNewlines();
    if (i >= src.length) break;
    if (src[i] === '[') {
      const arrayTable = src[i + 1] === '[';
      i += arrayTable ? 2 : 1;
      const parts = dottedKey();
      skipWs();
      if (!src.startsWith(arrayTable ? ']]' : ']', i)) throw err('expected ] closing the table header');
      i += arrayTable ? 2 : 1;
      expectEol();
      let t = root;
      for (const p of parts.slice(0, -1)) t = descend(t, p);
      const last = parts[parts.length - 1];
      if (arrayTable) {
        if (!(last in t)) t[last] = [];
        if (!Array.isArray(t[last])) throw err(`${parts.join('.')} is not an array of tables`);
        const fresh = {};
        t[last].push(fresh);
        current = fresh;
      } else {
        current = descend(t, last);
      }
      continue;
    }
    const k = dottedKey();
    skipWs();
    if (src[i] !== '=') throw err('expected =');
    i += 1;
    assign(current, k, value());
    expectEol();
  }
  return root;
}

/** The raw text of one `[header]` table: from its header line to the next header, comments included. */
export function tableText(text, header) {
  const src = text.replace(/\r\n/g, '\n').split('\n');
  const start = src.findIndex((l) => l.trim() === `[${header}]`);
  if (start < 0) return null;
  let end = src.length;
  for (let j = start + 1; j < src.length; j += 1) {
    if (/^\s*\[/.test(src[j])) {
      end = j;
      break;
    }
  }
  return src.slice(start, end).join('\n').trimEnd();
}
