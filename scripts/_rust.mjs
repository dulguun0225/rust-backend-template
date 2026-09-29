// A Rust lexer for the source rules: enough to tell code from comments and string literals, find attributes,
// macro invocations and `fn` bodies. It is not a parser; each rule that uses it names what it cannot see.
//
// tokens(src) -> [{ kind, text, value, line, start, end }]
//   kind: 'ident' | 'lifetime' | 'string' | 'char' | 'number' | 'punct'
//   value: the decoded contents of a string literal (escapes processed for normal strings, raw as written for
//   raw strings); undefined otherwise. Comments are dropped, doc comments included.

const IDENT_START = /[\p{L}_]/u;
const IDENT_CONT = /[\p{L}\p{N}_]/u;

export function tokens(src) {
  const out = [];
  let i = 0;
  let line = 1;
  const n = src.length;
  const advance = (to) => {
    for (let k = i; k < to; k += 1) if (src[k] === '\n') line += 1;
    i = to;
  };
  while (i < n) {
    const c = src[i];
    if (c === '\n' || c === ' ' || c === '\t' || c === '\r') {
      advance(i + 1);
      continue;
    }
    if (src.startsWith('//', i)) {
      const end = src.indexOf('\n', i);
      advance(end < 0 ? n : end);
      continue;
    }
    if (src.startsWith('/*', i)) {
      let depth = 1;
      let k = i + 2;
      while (k < n && depth > 0) {
        if (src.startsWith('/*', k)) {
          depth += 1;
          k += 2;
        } else if (src.startsWith('*/', k)) {
          depth -= 1;
          k += 2;
        } else k += 1;
      }
      advance(k);
      continue;
    }
    const startLine = line;
    // raw strings: r"..", r#".."#, br"..", cr".."
    const raw = /^(?:b|c)?r(#*)"/.exec(src.slice(i, i + 300));
    if (raw) {
      const hashes = raw[1];
      const open = i + raw[0].length;
      const close = src.indexOf(`"${hashes}`, open);
      if (close < 0) throw new Error(`unterminated raw string at line ${line}`);
      const end = close + 1 + hashes.length;
      out.push({ kind: 'string', text: src.slice(i, end), value: src.slice(open, close), line: startLine, start: i, end });
      advance(end);
      continue;
    }
    // strings: "..", b"..", c".."
    const quoted = /^(?:b|c)?"/.exec(src.slice(i, i + 2));
    if (quoted) {
      let k = i + quoted[0].length;
      let value = '';
      while (k < n && src[k] !== '"') {
        if (src[k] === '\\') {
          const [text, len] = unescape(src, k);
          value += text;
          k += len;
        } else {
          value += src[k];
          k += 1;
        }
      }
      if (k >= n) throw new Error(`unterminated string at line ${line}`);
      out.push({ kind: 'string', text: src.slice(i, k + 1), value, line: startLine, start: i, end: k + 1 });
      advance(k + 1);
      continue;
    }
    // char literals and lifetimes
    if (c === "'" || (c === 'b' && src[i + 1] === "'")) {
      const q = c === 'b' ? i + 1 : i;
      if (src[q + 1] === '\\') {
        const [, len] = unescape(src, q + 1);
        const end = q + 1 + len + 1;
        out.push({ kind: 'char', text: src.slice(i, end), line: startLine, start: i, end });
        advance(end);
        continue;
      }
      const cp = src.codePointAt(q + 1);
      const width = cp > 0xffff ? 2 : 1;
      if (src[q + 1 + width] === "'") {
        const end = q + 2 + width;
        out.push({ kind: 'char', text: src.slice(i, end), line: startLine, start: i, end });
        advance(end);
        continue;
      }
      if (c === "'") {
        let k = i + 1;
        while (k < n && IDENT_CONT.test(src[k])) k += 1;
        out.push({ kind: 'lifetime', text: src.slice(i, k), line: startLine, start: i, end: k });
        advance(k);
        continue;
      }
    }
    if (IDENT_START.test(c) || (c === 'r' && src[i + 1] === '#' && IDENT_START.test(src[i + 2] ?? ''))) {
      let k = c === 'r' && src[i + 1] === '#' ? i + 2 : i + 1;
      while (k < n && IDENT_CONT.test(src[k])) k += 1;
      out.push({ kind: 'ident', text: src.slice(i, k).replace(/^r#/, ''), line: startLine, start: i, end: k });
      advance(k);
      continue;
    }
    if (/[0-9]/.test(c)) {
      let k = i + 1;
      while (k < n && /[0-9A-Za-z_.]/.test(src[k]) && !(src[k] === '.' && (src[k + 1] === '.' || IDENT_START.test(src[k + 1] ?? '')))) k += 1;
      out.push({ kind: 'number', text: src.slice(i, k), line: startLine, start: i, end: k });
      advance(k);
      continue;
    }
    const two = src.slice(i, i + 2);
    const text = ['::', '->', '=>', '==', '!=', '<=', '>=', '&&', '||', '..'].includes(two) ? two : c;
    out.push({ kind: 'punct', text, line: startLine, start: i, end: i + text.length });
    advance(i + text.length);
  }
  return out;
}

function unescape(src, k) {
  const e = src[k + 1];
  const simple = { n: '\n', r: '\r', t: '\t', '\\': '\\', 0: '\0', "'": "'", '"': '"' };
  if (e in simple) return [simple[e], 2];
  if (e === 'x') return [String.fromCharCode(parseInt(src.slice(k + 2, k + 4), 16)), 4];
  if (e === 'u') {
    const close = src.indexOf('}', k);
    return [String.fromCodePoint(parseInt(src.slice(k + 3, close), 16)), close - k + 1];
  }
  if (e === '\n') {
    let j = k + 2;
    while (j < src.length && /\s/.test(src[j])) j += 1;
    return ['', j - k];
  }
  return [e, 2];
}

/** The index of the token closing the group opened at `open` ('(', '[' or '{'). */
export function closing(toks, open) {
  const pairs = { '(': ')', '[': ']', '{': '}' };
  const want = pairs[toks[open].text];
  let depth = 0;
  for (let k = open; k < toks.length; k += 1) {
    const t = toks[k];
    if (t.kind !== 'punct') continue;
    if (t.text in pairs) depth += 1;
    else if (t.text === ')' || t.text === ']' || t.text === '}') {
      depth -= 1;
      if (depth === 0) {
        if (t.text !== want) throw new Error(`mismatched ${toks[open].text} at line ${toks[open].line}`);
        return k;
      }
    }
  }
  throw new Error(`unclosed ${toks[open].text} at line ${toks[open].line}`);
}

/** Every attribute: { inner, text (the tokens inside the brackets, joined), toks, line }. */
export function attributes(toks) {
  const out = [];
  for (let k = 0; k < toks.length; k += 1) {
    if (toks[k].text !== '#') continue;
    const inner = toks[k + 1]?.text === '!';
    const open = inner ? k + 2 : k + 1;
    if (toks[open]?.text !== '[') continue;
    const close = closing(toks, open);
    const body = toks.slice(open + 1, close);
    out.push({ inner, toks: body, text: join(body), line: toks[k].line });
    k = close;
  }
  return out;
}

/** Tokens joined with the spacing rustfmt would leave around paths and punctuation, for messages and matching. */
export function join(toks) {
  let s = '';
  for (const t of toks) {
    const tight = t.kind === 'punct' && ['::', '(', ')', ',', '[', ']', '!', '.'].includes(t.text);
    const prevTight = s.endsWith('::') || s.endsWith('(') || s.endsWith('[') || s.endsWith('!') || s.endsWith('.');
    s += (s === '' || tight || prevTight ? '' : ' ') + t.text;
  }
  return s.replace(/,(?=\S)/g, ', ');
}

/** Every macro invocation `path::name!(…)`: { name, path, args (tokens inside), line }. */
export function macroCalls(toks) {
  const out = [];
  for (let k = 1; k < toks.length; k += 1) {
    if (toks[k].text !== '!' || toks[k - 1].kind !== 'ident') continue;
    const open = k + 1;
    if (!['(', '[', '{'].includes(toks[open]?.text)) continue;
    let p = k - 1;
    const segs = [toks[p].text];
    while (p >= 2 && toks[p - 1].text === '::' && toks[p - 2].kind === 'ident') {
      segs.unshift(toks[p - 2].text);
      p -= 2;
    }
    const close = closing(toks, open);
    out.push({ name: toks[k - 1].text, path: segs.join('::'), args: toks.slice(open + 1, close), line: toks[k].line, start: open, end: close });
  }
  return out;
}

/** Every `fn` item with a body: { name, line, start, end } as token indices of its braces. */
export function functions(toks) {
  const out = [];
  for (let k = 0; k < toks.length - 1; k += 1) {
    if (toks[k].text !== 'fn' || toks[k].kind !== 'ident' || toks[k + 1].kind !== 'ident') continue;
    let j = k + 2;
    let depth = 0;
    while (j < toks.length) {
      const t = toks[j].text;
      if (t === '(' || t === '[' || t === '<') depth += 1;
      else if (t === ')' || t === ']' || t === '>') depth -= 1;
      else if (depth <= 0 && (t === '{' || t === ';')) break;
      j += 1;
    }
    if (toks[j]?.text !== '{') continue;
    out.push({ name: toks[k + 1].text, line: toks[k].line, start: j, end: closing(toks, j) });
  }
  return out;
}
