/**
 * 🧠 AI Buffer Engine Studio - Core Logic (app.js)
 * 1. Engine Control (Commands)
 * 2. Data Management (CRUD)
 * 3. Bulk Intelligence (Drag & Drop)
 */

// --- 1. 엔진 제어 (Command Center) ---
async function runCommand(cmd) {
    const output = document.getElementById('cmd-output');
    output.innerText = `⏳ 명령 실행 중: ${cmd}...`;
    output.classList.remove('error', 'success');

    try {
        // 백엔드의 명령어 인터페이스 호출 (query string 방식)
        const res = await fetch(`/api/cmd?command=${cmd}`, { method: 'POST' });
        const result = await res.text();

        output.innerText = `[${new Date().toLocaleTimeString()}] ${result}`;
        output.classList.add('success');

        // 특정 명령어 실행 후 데이터 갱신
        if (['REWRITE', 'DEL', 'SET', 'SNAPSHOT'].includes(cmd)) {
            updateData();
        }
    } catch (err) {
        output.innerText = `❌ 에러: ${err.message}`;
        output.classList.add('error');
    }
}

// app.js 의 exportData 함수 수정
async function exportData() {
    // 🌟 현재 필터창에 입력된 검색어 가져오기
    const q = document.getElementById('filter-q').value;

    try {
        // 🌟 쿼리 스트링으로 필터값 전달
        const res = await fetch(`/api/export?q=${encodeURIComponent(q)}`);
        const blob = await res.blob();
        const url = window.URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;

        // 파일명에 필터 키워드 포함 (관리 용이)
        const fileName = q ? `filtered_${q}_${Date.now()}.jsonl` : `full_export_${Date.now()}.jsonl`;
        a.download = fileName;

        document.body.appendChild(a);
        a.click();
        window.URL.revokeObjectURL(url);
    } catch (err) {
        alert("내보내기 실패: " + err.message);
    }
}

// --- 2. 데이터 관리 (CRUD) ---

// 데이터 조회 및 테이블 업데이트 (통합 버전)
async function updateData() {
    const qInput = document.getElementById('filter-q');
    const q = qInput ? qInput.value : "";

    try {
        const res = await fetch(`/api/data?q=${encodeURIComponent(q)}`);
        const data = await res.json();

        let html = `<table><thead><tr>
            <th style="width: 10%">Type</th>
            <th style="width: 15%">Key</th>
            <th style="width: 40%">Value Content</th>
            <th style="width: 15%">Expire</th>
            <th style="width: 20%">Actions</th>
        </tr></thead><tbody>`;

        for (const [key, item] of Object.entries(data).sort()) {
            const typeClass = `type-${item.data_type.toLowerCase()}`;
            const expireText = item.expiry
                ? new Date(item.expiry * 1000).toLocaleTimeString()
                : '<span style="color:#aaa">Persist</span>';

            // 안전한 편집을 위한 값 처리
            const safeValue = item.value.replace(/'/g, "\\'");

            html += `<tr>
                <td><span class="badge ${typeClass}">${item.data_type}</span></td>
                <td><strong>${key}</strong></td>
                <td class="val-text">${item.value}</td>
                <td>${expireText}</td>
                <td>
                    <button class="btn-edit" onclick="editKey('${key}', '${safeValue}', '${item.data_type}')">편집</button>
                    <button class="btn-delete" onclick="deleteKey('${key}')">삭제</button>
                </td>
            </tr>`;
        }
        html += '</tbody></table>';
        document.getElementById('data-table').innerHTML = html;
    } catch (err) {
        console.error("Data fetch error:", err);
    }
}

async function addData() {
    const key = document.getElementById('key').value;
    const rawValue = document.getElementById('val-json').value;
    const type = document.getElementById('data-type').value;
    if(!key || !rawValue) return alert("키와 값을 모두 입력하세요.");

    let finalValue;
    try {
        // String 타입이 아니면 JSON 파싱 시도
        finalValue = (type === 'string') ? rawValue : JSON.parse(rawValue);
    } catch(e) {
        return alert("JSON 형식이 올바르지 않습니다. (예: [\"a\", \"b\"] 또는 {\"k\": \"v\"})");
    }

    const res = await fetch('/api/data', {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({ key, value: finalValue, data_type: type, ex: null })
    });

    if (res.ok) {
        document.getElementById('val-json').value = '';
        document.getElementById('key').value = '';
        updateData();
    }
}

async function deleteKey(key) {
    if(!confirm(`'${key}' 데이터를 영구 삭제할까요?`)) return;
    const res = await fetch(`/api/data/${encodeURIComponent(key)}`, { method: 'DELETE' });
    if(res.ok) updateData();
}

function editKey(key, value, type) {
    document.getElementById('key').value = key;
    document.getElementById('data-type').value = type.toLowerCase();

    // 디버그 출력물(Rust format!)을 JSON으로 역변환 시도
    let cleanVal = value;
    if(type !== 'String' && !value.startsWith('[') && !value.startsWith('{')) {
        cleanVal = value.replace(/\{/g, '{"').replace(/: /g, '": "').replace(/, /g, '", "').replace(/\}/g, '"}')
            .replace(/\[/g, '["').replace(/\]/g, '"]').replace(/", "/g, '", "');
    }
    document.getElementById('val-json').value = cleanVal;
    window.scrollTo({ top: 0, behavior: 'smooth' });
}

// --- 3. 벌크 지능 (Drag & Drop) ---

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
                    // 🌟 [천재적 혁신] CSV를 Hash 구조로 자동 변환
                    const lines = content.split('\n').map(l => l.trim()).filter(l => l.length > 0);
                    if (lines.length < 2) throw new Error("CSV 파일에 데이터가 부족합니다.");

                    // 1단계: 헤더 추출 (첫 줄)
                    const headers = lines[0].split(',').map(h => h.trim());

                    // 2단계: 각 행을 객체로 변환하여 Hash 타입으로 구성
                    dataToUpload = lines.slice(1).map((line, rowIdx) => {
                        const values = line.split(',').map(v => v.trim());
                        const rowObject = {};
                        headers.forEach((header, colIdx) => {
                            rowObject[header] = values[colIdx] || ""; // 값이 없으면 빈 문자열
                        });

                        return {
                            key: `${file.name}_${rowIdx + 1}`,
                            value: rowObject, // 🌟 문자열이 아닌 객체 전송
                            data_type: "hash" // 🌟 타입을 hash로 명시
                        };
                    });
                }
                else {
                    // TXT 파일: 기존의 의미 있는 문장 단위 쪼개기 유지
                    dataToUpload = content.split(/[.\n]/)
                        .filter(t => t.trim().length > 5)
                        .map((text, i) => ({
                            key: `${file.name}_${i}`,
                            value: text.trim(),
                            data_type: "string"
                        }));
                }

                document.getElementById('upload-status').innerHTML =
                    `<span style="color:var(--primary)">⏳ ${dataToUpload.length}개의 데이터 구조화 및 인덱싱 중...</span>`;

                const res = await fetch('/api/bulk', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify(dataToUpload)
                });

                if(res.ok) {
                    document.getElementById('upload-status').innerHTML =
                        `<span style="color:var(--success)">✅ 완료! ${dataToUpload.length}개의 데이터가 Hash 구조로 저장되었습니다.</span>`;
                    updateData();
                }
            } catch (err) {
                alert("파일 처리 실패: " + err.message);
                document.getElementById('upload-status').innerText = "";
            }
        };
        reader.readAsText(file);
    });
}

// --- 4. 유틸리티 및 초기화 ---

function updatePlaceholder() {
    const type = document.getElementById('data-type').value;
    const textarea = document.getElementById('val-json');
    const examples = {
        string: "일반 텍스트를 입력하세요...",
        list: '["항목1", "항목2", "항목3"]',
        hash: '{"필드": "값", "이름": "철수"}',
        set: '["유니크1", "유니크2"]'
    };
    textarea.placeholder = examples[type];
}

document.addEventListener('DOMContentLoaded', () => {
    updateData();
    setInterval(updateData, 5000); // 5초 간격 자동 갱신
});