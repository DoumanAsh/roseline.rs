extern crate regex;

use self::regex::Regex;

pub fn shell_split<'a>(string: &'a str) -> Result<Vec<&'a str>, String> {
    lazy_static! {
        static ref RE: Regex = Regex::new("\"[^\"]+\"|'[^']+'|\\S+").unwrap();
    }
    const QUOTES: &'static [char] = &['\'', '"'];
    let mut result: Vec<&'a str> = vec![];

    for mat in RE.find_iter(string) {
        let mat_str = mat.as_str();
        let arg = mat.as_str().trim_matches(QUOTES);
        let removed_quotes_count = mat_str.len() - arg.len();

        if removed_quotes_count != 0 && removed_quotes_count != 2 {
            return Err(format!("Badly quoted arguments after position {}", mat.start()));
        }

        result.push(arg)
    }

    Ok(result)
}


#[cfg(test)]
mod tests {
    use super::shell_split;

    #[test]
    fn should_shell_split_normal() {
        let text = "lolka arg 1  'arg 2' 'arg 3'";
        let expected = ["lolka", "arg", "1", "arg 2", "arg 3"];
        let result = shell_split(text).expect("To parse");

        assert_eq!(result.len(), expected.len());
        assert_eq!(result, &expected);
        println!("{:?}", result);
    }

    #[test]
    fn should_shell_split_bad_quotes() {
        let text = "lolka arg 1  'arg 2' \"arg 3";
        let result = shell_split(text);
        assert!(result.is_err())
    }

}
