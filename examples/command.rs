use anyhow::{Result, bail};
use std::time;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[tokio::main]
async fn main() -> Result<()> {
    let output = run_bash("ls -la", ".").await?;
    println!("command output: {output}");

    let output = run_bash_with_output("ls -la", ".").await?;
    println!("command output: {output}");

    Ok(())
}

pub async fn run_bash(cmd: &str, current_dir: &str) -> Result<String> {
    let timeout_duration = time::Duration::from_secs(DEFAULT_TIMEOUT_SECS);

    let mut child = Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .current_dir(current_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // 2. 接管输出流（Take 掉所有权，防止借用冲突）
    let mut stdout = child.stdout.take().expect("无法获取 stdout 管道");
    let mut stderr = child.stderr.take().expect("无法获取 stderr 管道");

    // 3. 创建一个异步任务来专门读取 stdout/stderr，防止阻塞子进程
    let stdout_handle = tokio::spawn(async move {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf).await;
        buf
    });

    let stderr_handle = tokio::spawn(async move {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf).await;
        buf
    });

    // 4. 对 child.wait() 进行超时控制
    // 注意：此时 stdout 正在被上面的异步任务源源不断地读取，缓冲区永远不会满，彻底避免了死锁！
    match timeout(timeout_duration, child.wait()).await {
        Ok(Ok(status)) => {
            // 在超时前成功执行完毕
            let stdout_content = stdout_handle.await.unwrap_or_default();

            if status.success() {
                println!("--- 命令执行成功 ---");
                println!("【标准输出】:\n{}", stdout_content);
                Ok(stdout_content)
            } else {
                let stderr_content = stderr_handle.await.unwrap_or_default();
                eprintln!("--- 命令执行失败，退出码: {:?} ---", status.code());
                eprintln!("【错误输出】:\n{}", stderr_content);
                Ok(stderr_content)
            }
        }
        Ok(Err(e)) => {
            let msg = format!("进程等待出错: {}", e);
            eprintln!("{}", msg);
            anyhow::bail!(msg)
        }
        Err(_) => {
            // 5. 触发了超时
            eprintln!("🛑 超出了预期时间! 正在强行终止子进程...");

            // 显式杀死进程
            let _ = child.kill().await;

            // 超时后，读取任务可能只读到了部分数据，也可以选择获取已经读到的部分
            let partial_stdout = stdout_handle.await.unwrap_or_default();
            println!("【超时前已收到的部分输出】:\n{}", partial_stdout);
            anyhow::bail!("命令执行超时！！！")
        }
    }
}

pub async fn run_bash_with_output(cmd: &str, current_dir: &str) -> Result<String> {
    let timeout_dur = time::Duration::from_secs(DEFAULT_TIMEOUT_SECS);

    let child = Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .current_dir(current_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // 超时发生时，child 自动 Drop，Tokio 自动杀进程
    let output = timeout(timeout_dur, child.wait_with_output())
        .await
        .map_err(|_| anyhow::anyhow!("命令执行超时 ({DEFAULT_TIMEOUT_SECS}s)"))??;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output
            .status
            .code()
            .map_or("未知".to_string(), |c| c.to_string());
        bail!("命令执行失败 (退出码 {code}): {stderr}")
    }
}
