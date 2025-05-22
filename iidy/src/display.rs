pub fn display_lines<I, S>(lines: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for line in lines {
        println!("{}", line.as_ref());
    }
}
