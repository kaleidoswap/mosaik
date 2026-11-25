/** One JSON line per log entry, on stderr. */

type Fields = Record<string, unknown>;

const write = (level: string, msg: string, fields?: Fields) =>
  process.stderr.write(JSON.stringify({ t: new Date().toISOString(), level, msg, ...fields }) + '\n');

export const log = {
  info: (msg: string, fields?: Fields) => write('info', msg, fields),
  warn: (msg: string, fields?: Fields) => write('warn', msg, fields),
  error: (msg: string, fields?: Fields) => write('error', msg, fields),
};
