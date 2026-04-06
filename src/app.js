/**
 * 🚀 AI Knowledge Lab - Core Logic (app.js)
 * Architecture: Shared-Nothing & Semantic Intelligence
 */

// --- 1. 엔진 제어 및 관제 (Command Center) ---

async function runCommand(cmd) {
    const output = document.getElementById('cmd-output');
    output.innerText = `⏳ 워커에게 명령 전달 중: ${cmd}...`;

    try {
        const res = await fetch(`/api/cmd?command=${cmd}`, { method: 'POST' });
        const result = await res.text();

        // 결과 가공: RESP 기호 정화 및 출력
        output.innerHTML = `<span style="color:#00ff00;">[${new Date().toLocaleTimeString()}]</span> ${result}`;

        // 데이터 변경 명령인 경우 테이블 즉시 갱신
        if (['REWRITE', 'DEL', 'SET', 'SNAPSHOT', 'BULK'].includes(cmd)) {
            updateData();
        }

        // 샤드 애니메이션 시뮬레이션 (워커가 일하고 있음을 시각화)
        flashShards();
    } catch (err) {
        output.innerHTML = `<span style="color:#ff4444;">❌ 에러: ${err.message}</span>`;
    }
}

// 🌟 [신규] AI 시맨틱 검색 로직
async function runAiSearch() {
    const queryInput = document.getElementById('ai-query');
    const resultBox = document.getElementById('ai-results');
    const query = queryInput.value.trim();

    if (!query) return alert("질문 내용을 입력해주세요.");

    resultBox.innerHTML = "🧠 16개 샤드 워커가 의미를 분석하며 검색 중입니다...";

    try {
        // 백엔드의 SEARCH 명령어 호출 (K=5, Threshold=0.5 기본값 사용 가정)
        const res = await fetch(`/api/cmd?command=SEARCH&query=${encodeURIComponent(query)}`, { method: 'POST' });
        const data = await res.text();

        // 결과를 읽기 좋게 줄바꿈 처리하여 표시
        const formattedData = data.replace(/\n/g, '<br>');
        resultBox.innerHTML = `<div class="search-result-item">${formattedData}</div>`;

        flashShards(); // 검색 시에도 샤드들이 반응하게 함
    } catch (err) {
        resultBox.innerHTML = `<span style="color:#f44336;">검색 중 오류 발생: ${err.message}</span>`;
    }
}

// 🌟 [신규] 샤드 상태 시각화 애니메이션
function flashShards() {
    const dots = document.querySelectorAll('.shard-dot');
    dots.forEach(dot => {
        dot.style.background = '#34a853';
        dot.style.boxShadow = '0 0 10px #34a853';
        setTimeout(() => {
            dot.style.background = '#ccc';
            dot.style.boxShadow = 'none';
        }, 500 + Math.random() * 1000); // 워커별로 처리 시간이 다름을 표현
    });
}

// --- 2. 데이터 관리 (CRUD & Analysis) ---

async function updateData() {
    const qInput = document.getElementById('filter-q');
    const q = qInput ? qInput.value : "";

    try {
        const res = await fetch(`/api/data?q=${encodeURIComponent(q)}`);
        const data = await res.json();

        let html = `<table><thead><tr>
            <th style="width: 10%">Type</th>
            <th style="width: 15%">Key</th>
            <th style="width: 40%">Value (Hash Content)</th>
            <th style="width: 15%">Expiry</th>
            <th style="width: 20%">Actions</th>
        </tr></thead><tbody>`;

        for (const [key, item] of Object.entries(data).sort()) {
            const typeClass = `type-${item.data_type.toLowerCase()}`;
            const expireText = item.expiry
                ? new Date(item.expiry * 1000).toLocaleTimeString()
                : '<span style="color:#aaa">Persist</span>';

            // 값의 길이를 적절히 제한하여 UI 깨짐 방지
            const displayValue = item.value.length > 150 ? item.value.substring(0, 150) + "..." : item.value;
            const safeValue = displayValue.replace(/'/g, "\\'");

            html += `<tr>
                <td><span class="badge ${typeClass}">${item.data_type}</span></td>
                <td><strong>${key}</strong></td>
                <td class="val-text">${displayValue}</td>
                <td>${expireText}</td>
                <td>
                    <button class="btn-edit" onclick="editKey('${key}', '${safeValue}', '${item.data_type}')">✏️</button>
                    <button class="btn-delete" onclick="deleteKey('${key}')">🗑️</button>
                </td>
            </tr>`;
        }
        html += '</tbody></table>';
        document.getElementById('data-table').innerHTML = html;
    } catch (err) {
        console.error("Fetch error:", err);
    }
}

async function exportData() {
    const q = document.getElementById('filter-q').value;
    try {
        const res = await fetch(`/api/export?q=${encodeURIComponent(q)}`);
        const blob = await res.blob();
        const url = window.URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = q ? `filtered_${q}_export.jsonl` : `full_brain_dump.jsonl`;
        document.body.appendChild(a);
        a.click();
        window.URL.revokeObjectURL(url);
    } catch (err) {
        alert("내보내기 실패");
    }
}

// --- 3. 벌크 데이터 지능형 주입 (CSV/JSON/TXT) ---

const dropZone = document.getElementById('drop-zone');
if (dropZone) {
    ['dragenter', 'dragover', 'dragleave', 'drop'].forEach(name => {
        dropZone.addEventListener(name, e => { e.preventDefault(); e.stopPropagation(); });
    });

    dropZone.addEventListener('drop', async (e) => {
        const files = e.dataTransfer.files;
        if (files.length === 0) return;

        const file = files[0];
        const reader = new FileReader();

        reader.onload = async (event) => {
            const content = event.target.result;
            let dataToUpload = [];

            try {
                if (file.name.endsWith('.json')) {
                    dataToUpload = JSON.parse(content);
                }
                else if (file.name.endsWith('.csv')) {
                    const lines = content.split('\n').map(l => l.trim()).filter(l => l.length > 0);
                    const headers = lines[0].split(',').map(h => h.trim());

                    dataToUpload = lines.slice(1).map((line, rowIdx) => {
                        const values = line.split(',').map(v => v.trim());
                        const rowObject = {};
                        headers.forEach((h, i) => rowObject[h] = values[i] || "");
                        return { key: `${file.name}_${rowIdx+1}`, value: rowObject, data_type: "hash" };
                    });
                } else {
                    dataToUpload = content.split(/[.\n]/).filter(t => t.trim().length > 5)
                        .map((text, i) => ({ key: `${file.name}_${i}`, value: text.trim(), data_type: "string" }));
                }

                document.getElementById('upload-status').innerHTML = `⏳ 16개 샤드로 분산 인덱싱 중... (${dataToUpload.length}건)`;

                const res = await fetch('/api/bulk', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify(dataToUpload)
                });

                if(res.ok) {
                    document.getElementById('upload-status').innerHTML = `<span style="color:#34a853;">✅ 완료! 데이터가 지능적으로 구조화되었습니다.</span>`;
                    updateData();
                    flashShards();
                }
            } catch (err) {
                alert("처리 실패: " + err.message);
            }
        };
        reader.readAsText(file);
    });
}

// --- 4. 유틸리티 (기존 로직 최적화 통합) ---

function updatePlaceholder() {
    const type = document.getElementById('data-type').value;
    const textarea = document.getElementById('val-json');
    const tips = { string: "텍스트 입력...", list: '["A", "B"]', hash: '{"키": "값"}', set: '["X", "Y"]' };
    textarea.placeholder = tips[type];
}

async function addData() {
    const key = document.getElementById('key').value.trim();
    const val = document.getElementById('val-json').value.trim();
    const type = document.getElementById('data-type').value;
    if (!key || !val) return alert("모든 정보를 입력하세요.");

    try {
        const value = (type === 'string') ? val : JSON.parse(val);
        const res = await fetch('/api/data', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({ key, value, data_type: type, ex: null })
        });
        if(res.ok) {
            updateData();
            flashShards();
            document.getElementById('val-json').value = '';
            document.getElementById('key').value = '';
        }
    } catch(e) { alert("데이터 형식이 올바르지 않습니다."); }
}

async function deleteKey(key) {
    if(!confirm(`'${key}'를 삭제할까요?`)) return;
    const res = await fetch(`/api/data/${encodeURIComponent(key)}`, { method: 'DELETE' });
    if(res.ok) updateData();
}

function editKey(key, val, type) {
    document.getElementById('key').value = key;
    document.getElementById('data-type').value = type.toLowerCase();
    document.getElementById('val-json').value = val;
    window.scrollTo({ top: 0, behavior: 'smooth' });
}

// 초기화
document.addEventListener('DOMContentLoaded', () => {
    updateData();
    setInterval(updateData, 5000);
});