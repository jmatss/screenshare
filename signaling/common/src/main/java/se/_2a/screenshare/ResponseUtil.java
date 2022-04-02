package se._2a.screenshare;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Collections;

import com.amazonaws.client.builder.AwsClientBuilder.EndpointConfiguration;
import com.amazonaws.services.apigatewaymanagementapi.AmazonApiGatewayManagementApiClient;
import com.amazonaws.services.apigatewaymanagementapi.model.PostToConnectionRequest;
import com.amazonaws.services.lambda.runtime.LambdaLogger;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketEvent.RequestContext;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2WebSocketResponse;

import com.google.gson.Gson;

public class ResponseUtil {
    /**
     * Example: https://<API_ID>.execute-api.eu-north-1.amazonaws.com/<STAGE>
     */
    private static String createEndpointUrl(String gatewayDomain, String stage) {
        return "https://" + gatewayDomain + "/" + stage;
    }

    /**
     * Sends the `message` through API Gateway to the websocket connection with
     * ID `connectionId`.
     */
    public static void pushToClient(String connectionId, String msg, RequestContext requestContext,
                                    LambdaLogger logger)
    {
        String region = System.getenv(Constants.AWS_REGION_ENV);
        String gatewayDomain = requestContext.getDomainName();
        String stage = requestContext.getStage();
        String endpointUrl = createEndpointUrl(gatewayDomain, stage);

        String logmsg = "Pushing message to client with ID \"" + connectionId + "\".";
        logmsg += " Endpoint URL: " + endpointUrl + ", message: " + msg;
        logger.log(logmsg);

        EndpointConfiguration endpointConfiguration = new EndpointConfiguration(endpointUrl, region);
        PostToConnectionRequest postRequest = new PostToConnectionRequest()
            .withConnectionId(connectionId)
            .withData(ByteBuffer.wrap(msg.getBytes(StandardCharsets.UTF_8)));

        AmazonApiGatewayManagementApiClient.builder()
            .withEndpointConfiguration(endpointConfiguration)
            .build()
            .postToConnection(postRequest);
    }

    public static APIGatewayV2WebSocketResponse createSuccess(Object content, LambdaLogger logger) {
        return createSuccess(content, "", logger);
    }

    public static APIGatewayV2WebSocketResponse createSuccess(Object content, String type, LambdaLogger logger) {
        return createResponse(content, type, 200, logger);
    }

    public static APIGatewayV2WebSocketResponse createError(Object content, LambdaLogger logger) {
        return createError(content, "", logger);
    }

    public static APIGatewayV2WebSocketResponse createError(Object content, String type, LambdaLogger logger) {
        return createResponse(content, type, 500, logger);
    }

    private static APIGatewayV2WebSocketResponse createResponse(Object content, String type, int statusCode,
                                                                LambdaLogger logger)
    {
        String msg = new Gson().toJson(content);

        StringBuilder sb = new StringBuilder("{");
        sb.append("\"action\": \"" + Constants.REPONSE + "\", ");
        sb.append("\"type\": \"" + type + "\", ");
        sb.append("\"data\": " + msg);
        sb.append("}");

        String reponseMsg = sb.toString();
        logger.log("Response (" + statusCode + "): " + reponseMsg);

        APIGatewayV2WebSocketResponse response = new APIGatewayV2WebSocketResponse();
        response.setIsBase64Encoded(false);
        response.setStatusCode(statusCode);
        response.setHeaders(Collections.singletonMap("content-type", "application/json"));
        response.setBody(reponseMsg);

        return response;
    }
}
