use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

/// BM25 스코어링을 위한 토크나이저 및 연산기
/// 외부 무거운 형태소 분석기 없이 `unicode-segmentation`의
/// 단어 경계 분할(word boundary) 알고리즘을 사용합니다.
pub struct Bm25Tokenizer {
    k1: f64,
    b: f64,
}

impl Default for Bm25Tokenizer {
    fn default() -> Self {
        Self {
            k1: 1.2, // 보통 1.2 ~ 2.0 사이의 값
            b: 0.75, // 문서 길이에 대한 가중치 (0.75 표준)
        }
    }
}

pub struct DocumentIndex {
    pub id: String,
    pub term_freqs: HashMap<String, usize>,
    pub total_terms: usize,
}

impl Bm25Tokenizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// 텍스트를 unicode segment 기준의 단어(단어 경계)로 분할하고 소문자로 변환합니다.
    pub fn tokenize(text: &str) -> Vec<String> {
        text.unicode_words()
            .map(|word| word.to_lowercase())
            .filter(|w| !w.trim().is_empty())
            .collect()
    }

    /// 단일 문서를 인덱싱하기 쉬운 형태로 변환합니다. (Term Frequency 계산)
    pub fn build_doc_index(id: String, text: &str) -> DocumentIndex {
        let tokens = Self::tokenize(text);
        let total_terms = tokens.len();
        let mut term_freqs = HashMap::new();

        for token in tokens {
            *term_freqs.entry(token).or_insert(0) += 1;
        }

        DocumentIndex {
            id,
            term_freqs,
            total_terms,
        }
    }

    /// 여러 대상 문서의 TF-IDF 및 BM25 점수를 계산하여 (문서 ID, 점수) 벡터를 반환합니다.
    pub fn score_documents(
        &self,
        query: &str,
        documents: &[DocumentIndex],
    ) -> Vec<(String, f64)> {
        let query_tokens = Self::tokenize(query);
        let n = documents.len() as f64; // 전체 문서 수

        if n == 0.0 || query_tokens.is_empty() {
            return Vec::new();
        }

        // 1. 전체 문서들의 평균 길이 (avgdl)
        let total_length: usize = documents.iter().map(|d| d.total_terms).sum();
        let avgdl = total_length as f64 / n;

        // 2. 검색어 토큰별 문서 빈도수 (DF: Document Frequency 계산)
        // df[term] = 해당 term을 포함하고 있는 문서의 개수
        let mut df: HashMap<String, usize> = HashMap::new();
        for token in &query_tokens {
            if !df.contains_key(token) {
                let count = documents
                    .iter()
                    .filter(|d| d.term_freqs.contains_key(token))
                    .count();
                df.insert(token.clone(), count);
            }
        }

        // 3. 각 문서별 BM25 점수 계산
        let mut scores = Vec::with_capacity(documents.len());

        for doc in documents {
            let mut doc_score = 0.0;
            let doc_len = doc.total_terms as f64;

            for token in &query_tokens {
                if let Some(freq_count) = doc.term_freqs.get(token) {
                    let tf = *freq_count as f64;
                    // 문서 빈도수
                    let doc_freq = *df.get(token).unwrap_or(&0) as f64;
                    
                    // 역문서 빈도 (IDF)
                    let idf = ((n - doc_freq + 0.5) / (doc_freq + 0.5) + 1.0).ln();

                    // 문서 길이 정규화 공식
                    let term_saturation =
                        (tf * (self.k1 + 1.0)) / (tf + self.k1 * (1.0 - self.b + self.b * (doc_len / avgdl)));

                    // idf 값이 음수가 나오지 않도록 보정 (BM25+ 등에서 활용)
                    let adjusted_idf = if idf < 0.0 { 0.01 } else { idf };

                    doc_score += adjusted_idf * term_saturation;
                }
            }
            scores.push((doc.id.clone(), doc_score));
        }

        scores
    }
}
