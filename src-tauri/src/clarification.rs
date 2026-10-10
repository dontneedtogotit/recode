use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionBlocking {
    Blocking,
    NonBlocking,
}

impl QuestionBlocking {
    pub fn is_blocking(&self) -> bool {
        matches!(self, QuestionBlocking::Blocking)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ClarificationQuestion {
    pub id: String,
    pub question: String,
    pub category: String,
    pub priority: i32,
    pub blocking: QuestionBlocking,
    pub rationale: String,
    pub options: Vec<String>,
    pub suggested_answer: Option<String>,
}

impl ClarificationQuestion {
    pub fn new(id: impl Into<String>, question: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            question: question.into(),
            category: String::new(),
            priority: 0,
            blocking: QuestionBlocking::NonBlocking,
            rationale: String::new(),
            options: Vec::new(),
            suggested_answer: None,
        }
    }

    pub fn blocking(mut self) -> Self {
        self.blocking = QuestionBlocking::Blocking;
        self
    }

    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn options(mut self, options: Vec<String>) -> Self {
        self.options = options;
        self
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrderedClarification {
    pub questions: Vec<ClarificationQuestion>,
}

impl OrderedClarification {
    pub fn new(questions: Vec<ClarificationQuestion>) -> Self {
        Self { questions }
    }

    /// Reorder clarification questions into a deterministic display order:
    ///   blocking first (descending priority), then non-blocking
    ///   (descending priority). Stable: equal-priority items keep their
    ///   input order.
    ///
    /// Returns a new value; the input is not mutated.
    pub fn reordered(&self) -> Vec<ClarificationQuestion> {
        if self.questions.is_empty() {
            return Vec::new();
        }

        let mut decorated: Vec<(usize, i32, bool, ClarificationQuestion)> = Vec::new();
        for (index, question) in self.questions.iter().cloned().enumerate() {
            let priority = if question.priority == 0 { 0 } else { question.priority };
            let blocking = question.blocking.is_blocking();
            decorated.push((index, priority, blocking, question));
        }

        decorated.sort_by(|left, right| {
            let blocking_diff = (right.2 as i32) - (left.2 as i32);
            if blocking_diff != 0 {
                return blocking_diff.cmp(&0);
            }

            let priority_diff = right.1.cmp(&left.1);
            if priority_diff != std::cmp::Ordering::Equal {
                return priority_diff;
            }

            left.0.cmp(&right.0)
        });

        decorated.into_iter().map(|tuple| tuple.3).collect()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClarificationQueue {
    pub pending: Vec<ClarificationQuestion>,
    pub answered: HashMap<String, String>,
}

impl ClarificationQueue {
    pub fn new(pending: Vec<ClarificationQuestion>) -> Self {
        Self {
            pending,
            answered: HashMap::new(),
        }
    }

    pub fn next(&self) -> Option<&ClarificationQuestion> {
        if self.pending.is_empty() {
            None
        } else {
            Some(&self.pending[0])
        }
    }

    pub fn answer(&mut self, question_id: impl Into<String>, answer: impl Into<String>) {
        let question_id = question_id.into();
        let answer = answer.into();

        if self
            .pending
            .iter()
            .any(|question| question.id == question_id)
        {
            self.answered.insert(question_id.clone(), answer);
            self.pending.retain(|question| question.id != question_id.clone());
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn answered_count(&self) -> usize {
        self.answered.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_questions() -> Vec<ClarificationQuestion> {
        vec![
            ClarificationQuestion::new("q3", "Feature priority?")
                .blocking()
                .priority(2),
            ClarificationQuestion::new("q1", "Target framework?"),
            ClarificationQuestion::new("q2", "Authentication method?")
                .blocking()
                .priority(1),
        ]
    }

    #[test]
    fn reorder_places_blocking_before_non_blocking() {
        let ordered = OrderedClarification::new(sample_questions());
        let reordered = ordered.reordered();

        assert_eq!(reordered[0].id, "q3");
        assert_eq!(reordered[1].id, "q2");
        assert_eq!(reordered[2].id, "q1");
    }

    #[test]
    fn reorder_sorts_higher_priority_first() {
        let ordered = OrderedClarification::new(sample_questions());
        let reordered = ordered.reordered();

        let blocking: Vec<&ClarificationQuestion> = reordered
            .iter()
            .filter(|q| q.blocking.is_blocking())
            .collect();
        assert_eq!(blocking.len(), 2);
        assert_eq!(blocking[0].id, "q3");
        assert_eq!(blocking[0].priority, 2);
        assert_eq!(blocking[1].id, "q2");
        assert_eq!(blocking[1].priority, 1);
    }

    #[test]
    fn reorder_preserves_input_order_for_equal_priority() {
        let questions = vec![
            ClarificationQuestion::new("q1", "First non-blocking"),
            ClarificationQuestion::new("q2", "Second non-blocking"),
        ];
        let ordered = OrderedClarification::new(questions);
        let reordered = ordered.reordered();

        assert_eq!(reordered[0].id, "q1");
        assert_eq!(reordered[1].id, "q2");
    }

    #[test]
    fn reorder_returns_empty_for_empty_input() {
        let ordered = OrderedClarification::new(Vec::new());
        assert!(ordered.reordered().is_empty());
    }

    #[test]
    fn queue_answers_move_question_from_pending_to_answered() {
        let questions = vec![
            ClarificationQuestion::new("q1", "Framework?"),
            ClarificationQuestion::new("q2", "Priority?").blocking(),
        ];
        let mut queue = ClarificationQueue::new(questions);

        assert_eq!(queue.pending_count(), 2);
        assert_eq!(queue.answered_count(), 0);
        assert_eq!(queue.next().map(|q| q.id.as_str()), Some("q1"));

        queue.answer("q1", "React");
        assert_eq!(queue.pending_count(), 1);
        assert_eq!(queue.answered_count(), 1);
        assert_eq!(queue.next().map(|q| q.id.as_str()), Some("q2"));
        assert_eq!(queue.answered.get("q1"), Some(&"React".to_string()));
    }

    #[test]
    fn queue_answer_returns_none_when_empty() {
        let mut queue = ClarificationQueue::new(Vec::new());
        assert!(queue.next().is_none());
        queue.answer("missing", "ignored");
        assert!(queue.answered.is_empty());
    }
}
