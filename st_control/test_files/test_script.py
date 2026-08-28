# 测试文档

## 概述
这是一个用于测试的Markdown文档。

## 内容
- **粗体**测试
- *斜体*测试
- 代码测试

## 表格

| 项目 | 值 |
|------|-----|
| 测试1 | 通过 |
| 测试2 | 通过 |

> 这是一段引用文本

`javascript
function hello() {
    console.log("Hello World");
}
`
"@ | Out-File -FilePath .\test_files\test_doc.md -Encoding utf8

# JSON 文件
'{"name": "测试数据", "version": "1.0", "items": [1, 2, 3]}' | Out-File -FilePath .\test_files\test_data.json -Encoding utf8

# Python 文件
@"
def hello():
    """Hello World"""
    print("Hello World")
    
class TestClass:
    def __init__(self, name):
        self.name = name
    
    def say_hi(self):
        print(f"Hi {self.name}")
