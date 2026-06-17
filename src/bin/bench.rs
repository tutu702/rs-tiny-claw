use rs_tiny_claw::eval::benchmark::{BenchmarkRunner, TestCase};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let base_url = std::env::var("OPENAI_BASE_URL")?;
    let model = std::env::var("LLM_MODEL")?;
    let api_key = std::env::var("LLM_API_KEY")?;

    // 构建一套微型评测集
    let testcases = vec![
        TestCase::new(
            "test_001_edit",
            "测试模糊替换工具的准确性",
            // 准备靶机：生成一个有错误的 json 文件
            r#"echo '{"name": "tiny-claw", "version": "v1.0.0"}' > config.json"#,
            // 考题：要求修改版本号
            r#"当前目录下有一个 config.json。请你使用 edit_file 工具，将其中的 version 从 v1.0.0 改为 v2.0.0。不要做其他多余操作。"#,
            // 判卷脚本：使用 grep 检查文件是否包含 v2.0.0
            r#"grep '"version": "v2.0.0"' config.json"#,
            0,
        ),
        TestCase::new(
            "test_002_code_gen",
            "测试代码阅读与创建新文件的综合能力",
            // 准备靶机：生成一个简单的乘法函数
            r#"echo 'package math\n\nfunc Multiply(a, b int) int {\n\treturn a * b\n}' > math.go"#,
            // 考题：要求 Agent 根据刚才的代码，自己去写一份单元测试
            r#"当前目录下有一个 math.go。请你仔细阅读它，然后在同级目录下，帮我写一个规范的单元测试文件 math_test.go，用来测试 Multiply 函数。请务必包含正常的测试用例。"#,
            // 判卷脚本：直接运行 go test！如果不通过则直接 0 分。
            r#"go mod init bench && go test -v ."#,
            0,
        ),
    ];

    // 启动跑分执行器！
    let runner = BenchmarkRunner::new(&base_url, &model, &api_key);
    runner.run_suite(testcases).await?;

    Ok(())
}
