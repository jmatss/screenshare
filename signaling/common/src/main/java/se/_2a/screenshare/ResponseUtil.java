package se._2a.screenshare;

import com.amazonaws.services.lambda.runtime.LambdaLogger;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketResponse;
import com.google.gson.Gson;

public class ResponseUtil {
    public static APIGatewayV2WebSocketResponse createSuccess(Object content, LambdaLogger logger) {
        return createResponse(content, 200, logger);
    }

    public static APIGatewayV2WebSocketResponse createError(Object content, LambdaLogger logger) {
        return createResponse(content, 500, logger);
    }

    private static APIGatewayV2WebSocketResponse createResponse(Object content, int statusCode, LambdaLogger logger) {
        String msg = new Gson().toJson(content);
        logger.log("Response (" + statusCode + "): " + msg);

        APIGatewayV2WebSocketResponse response = new APIGatewayV2WebSocketResponse();
        response.setBody(msg);
        response.setStatusCode(statusCode);

        return response;
    }
}
