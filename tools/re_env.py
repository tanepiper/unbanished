"""Where the reverse-engineering tools are, for the Python tools (tools/re-env.sh for shell).

Reads `.env` at the repository root (tools/re-setup.sh writes it), then the environment, then
the usual install places. `ghidra()` is the Ghidra folder, `analyze_headless()` its headless
launcher, `java_home()` a JDK 21; each is None when not found.
"""
import glob
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def _dotenv():
    values = {}
    path = os.path.join(ROOT, '.env')
    if os.path.exists(path):
        for line in open(path, encoding='utf-8'):
            key, sep, value = line.strip().partition('=')
            if sep and not key.startswith('#'):
                values[key.strip()] = value.strip().strip('"\'')
    return values


def _setting(name):
    return os.environ.get(name) or _dotenv().get(name)


def ghidra():
    for d in [_setting('GHIDRA_INSTALL_DIR'), '/opt/homebrew/opt/ghidra/libexec',
              '/usr/local/opt/ghidra/libexec', '/opt/ghidra', '/usr/share/ghidra',
              *sorted(glob.glob(os.path.expanduser('~/ghidra_*_PUBLIC')))]:
        if d and os.access(os.path.join(d, 'support', 'analyzeHeadless'), os.X_OK):
            return d
    return None


def analyze_headless():
    g = ghidra()
    return os.path.join(g, 'support', 'analyzeHeadless') if g else None


def java_home():
    for d in [_setting('JAVA_HOME'), '/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home',
              '/usr/lib/jvm/java-21-openjdk-amd64', '/usr/lib/jvm/java-21-openjdk',
              '/usr/lib/jvm/temurin-21-jdk-amd64']:
        if d and os.access(os.path.join(d, 'bin', 'java'), os.X_OK):
            return d
    return None
