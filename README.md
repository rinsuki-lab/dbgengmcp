# dbgengmcp

DbgEng.dll を直接バックエンドとして利用した、WinDbg用のMCPサーバーです。

## 提供する(予定の)ツール

* [x] `connect`: リモートの WinDbg に接続します。
  * `remote`: 接続文字列
* [x] `execute_command`: WinDbg コマンドを実行し、結果を得ます。WinDbg に接続できていない場合はエラーが帰ります。
  * `command`: 実行したいコマンド
* [x] `break_program`: 現在アタッチしているプログラムを Break します (実装予定)。
* [x] `disconnect`: 接続していた WinDbg から切断します。WinDbgに接続できていない場合はエラーが帰ります。
  * (パラメータなし)

### その他 TODO

* [ ] `execute_command` で progress 通知に対応し、(シンボルのリモートサーバーへの問い合わせなどの)長い時間かかるコマンド実行をタイムアウトさせない

## セットアップ

1. `dbgengmcp.exe` をビルドします。
1. https://www.nuget.org/packages/Microsoft.Debugging.Platform.DbgEng の `Download package` をクリックし、ダウンロードされた .nuget ファイルをzipファイルとして展開します。
  * `C:\\Windows\\System32\\dbgeng.dll` は WinDbg に接続できません
1. 先程のzipファイル内にある content/CPUアーキテクチャ名/ 内の DLL 郡を `dbgengmcp.exe` と同じフォルダにコピーします。
1. `dbgengmcp.exe --bind localhost:お好きなポート` で MCP サーバーを起動します。
1. あなたが使っているLLMエージェント内で、Transport は Streamable HTTP、URL は `http://localhost:先程のポート/mcp` を指定します。
1. 試しにツールが認識されているかを確認し (LLMに聞く、エージェントにMCPのツール一覧機能がある場合はそれを見るなど)、認識されていれば成功です。

## 使い方

1. WinDbg 上で `.server` コマンドを使い、WinDbg をサーバーにします。
  * 詳しくは https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/remote-debugging-using-windbg などを参照ください
1. WinDbg が出力した接続文字列の後、`-remote` の後の文字列をコピーし、LLMにこの文字列を使って接続するよう依頼します。
1. 接続が成功すれば、LLMがこのMCPサーバーを経由し自由に WinDbg のコマンドを実行できるようになります。

## 既存手法との比較

当初私は https://github.com/svnscha/mcp-windbg (0.13.0) を用いていたのですが、このリモート WinDbg 連携はコマンド実行後に `.echo` を使って終了マーカーを出力させることでコマンドの終了を検知しているようで、LLM が `g` コマンドで実行を再開した際に次のコマンドがいつまでも実行されず、コマンドが終了したことが検知されないのでツールコールもタイムアウトしてしまう、という問題がありました。

これは恐らく当該プロジェクトがCLIインターフェースである `cdb.exe` を経由して WinDbg を呼び出している弊害と思われます。

そのため、このプロジェクトでは `cdb.exe` や WinDbg が内部的に利用している `DbgEng.dll` の `IDebugClient` 公開COMインターフェースを直接使用することで、このような変な制約なしでの呼び出しを可能にしました。

https://github.com/NadavLor/windbg-ext-mcp も検討しましたが、何か不便なことがあった時に C++ のプラグイン部分に手を加えるのは面倒と判断し、自分でプロジェクトを書くに至りました。

## プロジェクトがこの形に至った背景

当初は、出力バイナリのリバースエンジニアリングが容易である(=悪意のあるコードがないかが比較的容易に検証可能である) C# での開発を試みましたが、どうも [CsWin32](https://github.com/microsoft/cswin32) が生成したCOMインターフェースに DbgEng.dll からもらったポインタを落としこむあたりで 0x80010103 (RPC_E_NOT_REGISTERED) エラーが発生し、私の貧弱な Windows 知識と LLM では解決が難しい状況に陥りました。

そこで、ふと思いたち、同じく Microsoft 公式でバインディング ([windows-rs](https://github.com/microsoft/windows-rs)) が用意されている Rust で試したところ、非常にうまくいったこと、また私がある程度 Rust への知識があり、さらに Windows の COM を Rust で呼びたい用事が今後ある (=この方法を学ぶインセンティブがある) ことが想定されたため、Rustで書くことにしました。

C++ で DbgEng.dll を利用したり、あるいは WinDbg のプラグインを書く手法も検討しましたが、私が C++ に不慣れであること、C++用のMCP公式SDKが存在せずサードパーティのものを採用するか別言語でプロキシを実装する必要があったこと、などからこの手法は見送りました。

また、本来は stdio transport を利用してMCPクライアントとやりとりする心積もりだったのですが、rmcp 側でステート不整合によるpanicが発生し、rmcp 側のコードを適当に grep したところ Windows 環境で CI が実行されておらず、恐らく正常に動作する見込みが薄そうであると判断したため、OS依存の要素が少ない  Streamable HTTP transport を採用したところ、こちらは問題なく動作したためこちらの採用に至っています。

ただし、このプロジェクトにおいて Streamable HTTP Transport を採用するのには以下のような問題があると考えており、気が向いたら stdio transport へ再挑戦したいと考えています。
* MCPサーバーを何らかの事情で再起動した際に全セッションがexpireするが、rmcpは知らないセッションIDを指定されるとエラーを返すため、このあたりのハンドリングが甘いMCPクライアントとの相性が悪くなりそう
  * LLMがMCPへの再接続を自律的に行える環境であれば問題ないかも?
  * stdio transport にすれば、少なくともセッション切れエラーは出なくなるはず (クラッシュ耐性などの別の問題はあるが)
* TCPのポート番号を決めるのは面倒くさい
* `http://localhost:*` にアクセスできるものなら誰でもこのMCPサーバーを叩けるのは如何なものか
