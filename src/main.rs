use scfg::Scfg;

fn main() {
    // an scfg document
    static SCFG_DOC: &str = r#"train "Shinkansen" {
        model "E5" {
            max-speed 320km/h
            weight 453.5t

            lines-served "Tōhoku" "Hokkaido"
        }

        model "E7" {
            max-speed 275km/h
            weight 540t

            lines-served "Hokuriku" "Jōetsu"
        }
    }"#;
    let doc = SCFG_DOC.parse::<Scfg>().expect("invalid document");

    // the above document can also be created with this builder style api
    let mut scfg = Scfg::new();
    let train = scfg
        .add("train")
        .append_param("Shinkansen")
        .get_or_create_child();
    let e5 = train.add("model").append_param("E5").get_or_create_child();
    e5.add("max-speed").append_param("320km/h");
    e5.add("weight").append_param("453.5t");
    e5.add("lines-served")
        .append_param("Tōhoku")
        .append_param("Hokkaido");
    let e7 = train.add("model").append_param("E7").get_or_create_child();
    e7.add("max-speed").append_param("275km/h");
    e7.add("weight").append_param("540t");
    e7.add("lines-served")
        .append_param("Hokuriku")
        .append_param("Jōetsu");

    println!("{:#?}", scfg.get("train").unwrap());

    assert_eq!(doc, scfg);
}
