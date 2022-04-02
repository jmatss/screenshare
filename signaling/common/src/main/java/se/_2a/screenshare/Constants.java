package se._2a.screenshare;

public class Constants {
    /**
     * Env set by default in all Lambda functions.
     */
    public static final String AWS_REGION_ENV = "AWS_REGION";

    public static final String DB_ACCESS_ID_ENV        = "DB_ACCESS_ID";
    public static final String DB_ACCESS_KEY_ENV       = "DB_ACCESS_KEY";
    public static final String TABLE_NAME_ENV          = "TABLE_NAME";
    public static final String ID_COLUMN_NAME_ENV      = "ID_COLUMN_NAME";
    public static final String TYPE_COLUMN_NAME_ENV    = "TYPE_COLUMN_NAME";

    public static final String ID      = "id";
    public static final String ACTION  = "action";
    public static final String DATA     = "data";
    public static final String PRODUCER = "producer";
    public static final String CONSUMER = "consumer";

    public static final String OFFER = "offer";
    public static final String ANSWER = "answer";
    public static final String CANDIDATE = "candidate";
    public static final String REPONSE = "response";
}
