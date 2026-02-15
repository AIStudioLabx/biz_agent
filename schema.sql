-- 对话会话表
CREATE TABLE IF NOT EXISTS chat_sessions (
    id CHAR(36) PRIMARY KEY,
    created_at DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
);

-- 对话消息表
CREATE TABLE IF NOT EXISTS chat_messages (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    session_id CHAR(36) NOT NULL,
    role VARCHAR(20) NOT NULL COMMENT 'user | assistant',
    content TEXT NOT NULL,
    created_at DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    INDEX idx_session_id (session_id)
);

-- Text-to-SQL 训练数据：问句-SQL 对，用于 RAG 检索
CREATE TABLE IF NOT EXISTS sql_training_data
(
    id
    BIGINT
    AUTO_INCREMENT
    PRIMARY
    KEY,
    question
    TEXT
    NOT
    NULL
    COMMENT
    '自然语言问句',
    sql_text
    TEXT
    NOT
    NULL
    COMMENT
    '对应的正确 SQL',
    source
    VARCHAR
(
    32
) NOT NULL DEFAULT 'manual' COMMENT 'manual | tool_memory',
    created_at DATETIME
(
    3
) NOT NULL DEFAULT CURRENT_TIMESTAMP
(
    3
),
    INDEX idx_created_at
(
    created_at
)
    );

-- 自定义表结构（DDL/说明）：未在当前库建表时也可让模型知晓表结构，生成 SQL 时与 information_schema 合并
CREATE TABLE IF NOT EXISTS schema_ddl
(
    id
    BIGINT
    AUTO_INCREMENT
    PRIMARY
    KEY,
    ddl_text
    TEXT
    NOT
    NULL
    COMMENT
    'CREATE TABLE 语句或表结构说明（列名、类型等）',
    comment
    VARCHAR
(
    255
) DEFAULT NULL COMMENT '备注，如：三方游戏记录表',
    created_at DATETIME
(
    3
) NOT NULL DEFAULT CURRENT_TIMESTAMP
(
    3
)
    );
