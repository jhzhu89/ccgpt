import { mkdir, readFile, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { basename, dirname, join } from "node:path";

const home = homedir();
const dataDir = join(home, ".local", "share", "ccgpt");

async function readIfPresent(path: string): Promise<string> {
  try {
    return await readFile(path, "utf8");
  } catch {
    return "";
  }
}

async function addSourceLine(profile: string, line: string): Promise<void> {
  const current = await readIfPresent(profile);
  if (current.split(/\r?\n/).includes(line)) return;

  const profileDir = dirname(profile);
  if (!existsSync(profileDir)) {
    await mkdir(profileDir, { recursive: true });
  }
  const separator = current.length > 0 && !current.endsWith("\n") ? "\n" : "";
  await writeFile(profile, `${current}${separator}${line}\n`);
}

function powerShellProfile(): string {
  const shell = Bun.which("pwsh") ?? Bun.which("powershell");
  if (!shell) throw new Error("PowerShell was not found");

  const result = Bun.spawnSync(
    [shell, "-NoProfile", "-Command", "$PROFILE.CurrentUserCurrentHost"],
    { stdout: "pipe", stderr: "pipe" },
  );
  if (result.exitCode !== 0) {
    throw new Error(result.stderr.toString().trim() || "PowerShell failed");
  }
  return result.stdout.toString().trim();
}

async function setupPowerShell(): Promise<string> {
  const integration = join(dataDir, "shell.ps1");
  await writeFile(
    integration,
    "function global:claude {\n  & ccgpt run @args\n}\n",
  );

  const profile = powerShellProfile();
  await addSourceLine(profile, '. "$HOME/.local/share/ccgpt/shell.ps1"');
  return profile;
}

async function setupPosixShell(): Promise<string> {
  const integration = join(dataDir, "shell.sh");
  await writeFile(integration, 'claude() {\n  command ccgpt run "$@"\n}\n');

  const shell = basename(process.env.SHELL ?? "bash");
  const profile = join(home, shell === "zsh" ? ".zshrc" : ".bashrc");
  await addSourceLine(profile, '. "$HOME/.local/share/ccgpt/shell.sh"');
  return profile;
}

await mkdir(dataDir, { recursive: true });
const profile =
  process.platform === "win32"
    ? await setupPowerShell()
    : await setupPosixShell();

process.stdout.write(
  `Claude shell integration installed in ${profile}\nOpen a new shell and run claude.\n`,
);
