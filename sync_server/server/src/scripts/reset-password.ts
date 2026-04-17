import process from "node:process";
import readline from "node:readline";
import { pathToFileURL } from "node:url";

import { env } from "../config/env.js";
import { DatabaseClient } from "../db/database.js";
import { emailSchema, passwordSchema } from "../services/auth.js";
import { hashPassword } from "../utils/crypto.js";

type PasswordPrompt = (label: string) => Promise<string>;

export async function resolveNewPassword(
  argvPassword: string | undefined,
  isInteractive: boolean,
  prompt: PasswordPrompt,
): Promise<string> {
  if (argvPassword !== undefined) {
    return passwordSchema.parse(argvPassword);
  }

  if (!isInteractive) {
    throw new Error("非交互环境且未提供新密码，请通过命令行参数传入");
  }

  const promptedPassword = await prompt("请输入新密码: ");
  return passwordSchema.parse(promptedPassword);
}

export function resetPasswordForEmail(database: DatabaseClient, email: string, newPassword: string) {
  const normalizedEmail = emailSchema.parse(email);
  const validatedPassword = passwordSchema.parse(newPassword);
  const user = database.findUserByEmail(normalizedEmail);

  if (!user) {
    throw new Error("用户不存在");
  }

  database.updateUserPassword(user.id, hashPassword(validatedPassword));
  const revokedSessions = database.deleteSessionsByUserId(user.id);

  return {
    email: normalizedEmail,
    revokedSessions,
  };
}

export function promptHiddenPassword(label: string): Promise<string> {
  return new Promise((resolve, reject) => {
    if (!process.stdin.isTTY || !process.stdout.isTTY) {
      reject(new Error("当前终端不支持交互输入，请通过命令行参数提供新密码"));
      return;
    }

    process.stdout.write(label);
    const rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
      terminal: true,
    });

    (rl as readline.Interface & { _writeToOutput(value: string): void })._writeToOutput = () => {};
    rl.question("", (answer) => {
      rl.close();
      process.stdout.write("\n");
      resolve(answer);
    });
  });
}

export async function main(argv = process.argv.slice(2)) {
  const [email, argvPassword] = argv;

  if (!email) {
    console.error("用法: npm run reset-password -- <email> [newPassword]");
    process.exitCode = 1;
    return;
  }

  const database = new DatabaseClient(env.dbPath, env.migrationsPath);

  try {
    const newPassword = await resolveNewPassword(argvPassword, Boolean(process.stdin.isTTY), promptHiddenPassword);
    const result = resetPasswordForEmail(database, email, newPassword);
    console.log(`密码已重置: ${result.email}，已撤销 ${result.revokedSessions} 个会话`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : "密码重置失败");
    process.exitCode = 1;
  } finally {
    database.close();
  }
}

const isEntrypoint = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (isEntrypoint) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : "密码重置失败");
    process.exitCode = 1;
  });
}
